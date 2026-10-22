#requires -Version 5
# Windows E2E smoke for the resident HUD shell (#26).
#
# Manual regression helper: it drives the real desktop (global hotkey, cursor, focus)
# and therefore runs only on Windows with an interactive session. Artifacts go to
# .local/ (gitignored). Usage:
#
#   npm run build
#   cargo build --release -p aetherhud --features tauri/custom-protocol
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-hud-shell.ps1
#
# Columns: PASS/FAIL, assertion, observed state. Exit code 1 when anything fails.
# Ordinary keystrokes land as IME composition when a Chinese input method is active,
# so the typing assertion uses KEYEVENTF_UNICODE injection (see docs/spec/hud-window-and-shell.md).
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int max);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtr")] public static extern IntPtr GetWindowLongPtr(IntPtr h, int index);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
"@

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$exe = Join-Path $root 'target\release\aetherhud.exe'
$cdp = Join-Path $root '.local\cdp-eval.mjs'
$shotDir = Join-Path $root '.local\shots'
New-Item -ItemType Directory -Force -Path $shotDir | Out-Null
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$WS_EX_TRANSPARENT = 0x20
$results = New-Object System.Collections.Generic.List[string]
$script:fails = 0

function Check([string]$name, [bool]$ok, [string]$detail) {
  $tag = if ($ok) { 'PASS' } else { 'FAIL'; $script:fails++ }
  $line = "$tag  $name  ($detail)"
  $results.Add($line) | Out-Null
  Write-Host $line
}
function Get-Rect([IntPtr]$h) {
  $r = New-Object W+RECT
  if (-not [W]::GetWindowRect($h, [ref]$r)) { throw 'GetWindowRect failed' }
  return $r
}
function Get-HudWindow([int]$pidValue) {
  $target = [uint32]$pidValue
  $script:found = [IntPtr]::Zero
  $cb = [W+EnumProc]{
    param($h, $l)
    $wpid = [uint32]0
    [W]::GetWindowThreadProcessId($h, [ref]$wpid) | Out-Null
    if ($wpid -eq $target -and [W]::IsWindowVisible($h)) {
      $t = New-Object System.Text.StringBuilder 256
      [W]::GetWindowText($h, $t, 256) | Out-Null
      if ($t.ToString() -eq 'AetherHUD') { $script:found = $h; return $false }
    }
    return $true
  }
  [W]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
  return $script:found
}
function Wait-Hud([int]$pidValue, [int]$timeoutMs = 15000) {
  $deadline = (Get-Date).AddMilliseconds($timeoutMs)
  while ((Get-Date) -lt $deadline) {
    $h = Get-HudWindow $pidValue
    if ($h -ne [IntPtr]::Zero) { return $h }
    Start-Sleep -Milliseconds 200
  }
  return [IntPtr]::Zero
}
function Is-OnScreen([IntPtr]$h) {
  $r = Get-Rect $h
  return ($r.Top -lt $screen.Bottom) -and ($r.Bottom -gt $screen.Top)
}
function Current-ExStyle([IntPtr]$h) {
  return [W]::GetWindowLongPtr($h, -20).ToInt64()
}
function Send-Hotkey {
  [W]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
  [W]::keybd_event(0xC0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
  [W]::keybd_event(0xC0, 0, 2, [UIntPtr]::Zero)
  [W]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
}
function Send-Esc {
  [W]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 20
  [W]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
}
# Unicode key injection (KEYEVENTF_UNICODE) keeps the assertion deterministic when a
# Chinese IME is the active input method; ordinary keystrokes land as IME composition
# text (verified separately, see the issue comment).
function Type-Text([string]$text) {
  foreach ($ch in $text.ToCharArray()) {
    $code = [byte][int][char]$ch
    [W]::keybd_event(0, $code, 4, [UIntPtr]::Zero)
    [W]::keybd_event(0, $code, 6, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
  }
}
function Hover([IntPtr]$h, [int]$dx, [int]$dy) {
  $r = Get-Rect $h
  [W]::SetCursorPos($r.Left + $dx, $r.Top + $dy) | Out-Null
  Start-Sleep -Milliseconds 350
}
function Hover-Raw([int]$x, [int]$y) {
  [W]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 350
}
function Cdp([string]$expression) {
  $raw = & node $cdp 9333 $expression
  if ($LASTEXITCODE -ne 0) { throw "cdp failed: $raw" }
  return ($raw | ConvertFrom-Json)
}
function Shot([string]$name) {
  $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $bmp = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bmp.Size)
  $g.Dispose()
  $bmp.Save((Join-Path $shotDir $name), [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9333'
$log = Join-Path $root '.local\smoke-app.log'
$proc = $null
$resident = $null
$second = $null
$hiddenSecond = $null

try {
  # --- 1. Manual launch summons a focused HUD ---
  $proc = Start-Process -FilePath $exe -PassThru -RedirectStandardError $log
  $hud = Wait-Hud $proc.Id
  Check 'manual launch maps the HUD window' ($hud -ne [IntPtr]::Zero) "hwnd=$hud"
  Start-Sleep -Milliseconds 1200
  Check 'first launch summons the HUD on screen' (Is-OnScreen $hud) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"
  Check 'summoned HUD owns the foreground' ([W]::GetForegroundWindow() -eq $hud) "foreground=$([W]::GetForegroundWindow())"
  Shot 'hud-summoned.png'

  # --- 2. 唤出即输入：no click, type immediately ---
  Check 'webview published hitboxes' ((Cdp "document.querySelectorAll('[data-hud-hitbox]').length") -ge 3) "hitboxes"
  $active = Cdp "document.activeElement && document.activeElement.tagName"
  Check 'search box is focused on summon' ($active -eq 'INPUT') "activeElement=$active"
  $fgBeforeTyping = [W]::GetForegroundWindow()
  Type-Text 'abc'
  Start-Sleep -Milliseconds 500
  $typed = Cdp "document.querySelector('input').value"
  Check 'typing right after summon lands in the search box' ($typed -eq 'abc') "value='$typed' foreground=$fgBeforeTyping hud=$hud"

  # --- 3. Click-through: empty desktop area transparent, search bar captured ---
  $r = Get-Rect $hud
  $emptyX = $r.Right - 40
  $emptyY = $r.Top + 300
  Hover-Raw $emptyX $emptyY
  $exEmpty = Current-ExStyle $hud
  $underEmpty = [W]::GetAncestor([W]::WindowFromPoint([W+POINT]@{X=$emptyX; Y=$emptyY}), 2)
  Check 'empty HUD area is click-through (WS_EX_TRANSPARENT)' (($exEmpty -band $WS_EX_TRANSPARENT) -ne 0) "exstyle=$exEmpty under=$underEmpty"

  Hover $hud 120 40
  $exBar = Current-ExStyle $hud
  $barPoint = [W+POINT]@{X=$r.Left + 120; Y=$r.Top + 40}
  $underBar = [W]::GetAncestor([W]::WindowFromPoint($barPoint), 2)
  Check 'search bar captures the cursor (no WS_EX_TRANSPARENT)' (($exBar -band $WS_EX_TRANSPARENT) -eq 0) "exstyle=$exBar under=$underBar"
  Check 'cursor over the search bar resolves to the HUD' ($underBar -eq $hud) "under=$underBar hud=$hud"

  # --- 4. Esc dismisses, focus returns to the previous app ---
  $previous = $null
  Send-Hotkey
  Start-Sleep -Milliseconds 700
  Check 'hotkey dismisses off screen' (-not (Is-OnScreen $hud)) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"
  $previous = [W]::GetForegroundWindow()
  Check 'focus leaves the HUD after dismiss' ($previous -ne $hud) "foreground=$previous"

  Send-Hotkey
  Start-Sleep -Milliseconds 700
  Check 'hotkey summons again' (Is-OnScreen $hud) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"
  Send-Esc
  Start-Sleep -Milliseconds 700
  Check 'Esc dismisses the HUD' (-not (Is-OnScreen $hud)) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"
  Check 'Esc hands focus back to the previous app' ([W]::GetForegroundWindow() -ne $hud) "foreground=$([W]::GetForegroundWindow())"

  # --- 5. Summon resets to S0 (query cleared) ---
  Send-Hotkey
  Start-Sleep -Milliseconds 700
  Check 'summon resets the query to S0' ((Cdp "document.querySelector('input').value") -eq '') "value"
  Shot 'hud-resummoned.png'

  # --- 6. WM_CLOSE (Alt+F4 / Cmd+W path) dismisses without quitting ---
  [W]::PostMessage($hud, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Milliseconds 800
  Check 'close request dismisses instead of quitting' (-not (Is-OnScreen $hud)) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"
  Check 'process alive after close request' (-not $proc.HasExited) "pid=$($proc.Id)"

  # --- 7. Second instance hands over; --hidden second instance stays quiet ---
  $second = Start-Process -FilePath $exe -PassThru
  $second.WaitForExit(10000) | Out-Null
  Start-Sleep -Milliseconds 900
  Check 'second instance exits instead of running again' ($second.HasExited) "exit=$($second.ExitCode)"
  Check 'second instance summons the existing HUD' (Is-OnScreen $hud) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"

  $hiddenSecond = Start-Process -FilePath $exe -ArgumentList '--hidden' -PassThru
  $hiddenSecond.WaitForExit(10000) | Out-Null
  Start-Sleep -Milliseconds 900
  Check '--hidden second instance exits' ($hiddenSecond.HasExited) "exit=$($hiddenSecond.ExitCode)"
  Check '--hidden second instance leaves the HUD as it was' (Is-OnScreen $hud) "rect=$((Get-Rect $hud).Left),$((Get-Rect $hud).Top)"

  # --- 8. --hidden first launch is resident only ---
  Stop-Process -Id $proc.Id -Force
  Start-Sleep -Milliseconds 800
  $proc = $null
  $resident = Start-Process -FilePath $exe -ArgumentList '--hidden' -PassThru -RedirectStandardError (Join-Path $root '.local\resident-app.log')
  $hud2 = Wait-Hud $resident.Id
  Start-Sleep -Milliseconds 1500
  Check '--hidden launch maps the window off screen' (($hud2 -ne [IntPtr]::Zero) -and (-not (Is-OnScreen $hud2))) "hwnd=$hud2 rect=$((Get-Rect $hud2).Left),$((Get-Rect $hud2).Top)"
  Check '--hidden process stays resident' (-not $resident.HasExited) "pid=$($resident.Id)"
}
catch {
  Check 'smoke script completed' $false "ERROR: $($_.Exception.Message)"
}
finally {
  foreach ($p in @($proc, $resident, $second, $hiddenSecond)) {
    if ($p -and -not $p.HasExited) { try { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } catch {} }
  }
  Get-Process aetherhud -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  [W]::SetCursorPos(($screen.Width / 2), ($screen.Height / 2)) | Out-Null
}
Write-Host ''
Write-Host '=== SUMMARY ==='
$results | ForEach-Object { Write-Host $_ }
Write-Host ''
Write-Host "fails: $script:fails"
Write-Host '=== app stderr ==='
if (Test-Path $log) { Get-Content $log }
if ($script:fails -gt 0) { exit 1 }