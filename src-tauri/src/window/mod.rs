//! The resident HUD window controller (#26).
//!
//! HUD visibility is *movement*, never `hide()`/`show()`: the window is shown once,
//! parked just below the monitor edge, and slides in/out with `set_position`
//! (#24 §8 / #12). This module owns the state machine, the slide animation, the
//! blur grace window, focus hand-back and the 50 ms click-through supervisor.

pub mod clickthrough;
pub mod geometry;
pub mod motion;
#[cfg(target_os = "macos")]
mod panel;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Monitor, WebviewWindow, WindowEvent};

use crate::events;
use crate::focus::{self, PreviousApp};
use crate::hotkey::{HotkeyState, HotkeyStatus};
use crate::platform;
use crate::startup::LaunchMode;
use crate::tray;

use self::clickthrough::{HitBoxRegistry, IgnoreCursor};
use self::geometry::MonitorBounds;
pub use self::motion::{DismissReason, Phase, SummonReason, Transition};
use self::motion::{HudMotion, SLIDE_DURATION};

/// A second process asked this instance to show the HUD before the webview was ready.
static PENDING_SUMMON: AtomicBool = AtomicBool::new(false);

/// Slide animation frame period. About 60 fps while the animation runs.
const ANIMATION_TICK: Duration = Duration::from_millis(12);

/// Safety net copied from T01: if the webview never reports ready, still park the
/// window off screen instead of leaving an unmapped, unreachable process.
const REVEAL_FALLBACK_DELAY: Duration = Duration::from_secs(3);

/// Which user gesture toggled the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserTrigger {
    Hotkey,
    Tray,
}

impl UserTrigger {
    fn summon_reason(self) -> SummonReason {
        match self {
            UserTrigger::Hotkey => SummonReason::Hotkey,
            UserTrigger::Tray => SummonReason::Tray,
        }
    }

    fn dismiss_reason(self) -> DismissReason {
        match self {
            UserTrigger::Hotkey => DismissReason::Hotkey,
            UserTrigger::Tray => DismissReason::Tray,
        }
    }
}

/// Snapshot of the shell state for the frontend (settings, diagnostics).
#[derive(Debug, Clone, Serialize)]
pub struct HudSnapshot {
    /// "hidden" | "showing" | "visible" | "dismissing"
    pub phase: &'static str,
    /// "show" (manual launch) | "resident" (--hidden)
    pub launch_mode: &'static str,
    pub hotkey: HotkeyStatus,
}

/// The single resident HUD.
pub struct HudWindow {
    app: AppHandle,
    window: WebviewWindow,
    launch_mode: LaunchMode,
    motion: Mutex<HudMotion>,
    /// Bumped to cancel an in-flight slide when a new transition starts.
    animation: Arc<AtomicU64>,
    ready: AtomicBool,
    previous_app: Mutex<Option<PreviousApp>>,
    /// Last on-screen top-left, so summoning returns the HUD where the user had it.
    last_visible: Mutex<Option<(f64, f64)>>,
    hitboxes: HitBoxRegistry,
    ignore_cursor: Mutex<IgnoreCursor>,
}

impl HudWindow {
    /// Builds the controller around the window declared in `tauri.conf.json` and
    /// installs the window event hook.
    pub fn attach(app: AppHandle, window: WebviewWindow, launch_mode: LaunchMode) -> Arc<Self> {
        let hud = Arc::new(Self {
            app: app.clone(),
            window: window.clone(),
            launch_mode,
            motion: Mutex::new(HudMotion::new()),
            animation: Arc::new(AtomicU64::new(0)),
            ready: AtomicBool::new(false),
            previous_app: Mutex::new(None),
            last_visible: Mutex::new(None),
            hitboxes: HitBoxRegistry::new(),
            ignore_cursor: Mutex::new(IgnoreCursor::new()),
        });

        let event_hud = Arc::clone(&hud);
        window.on_window_event(move |event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Alt+F4 / Cmd+W / the OS close button dismiss, never quit (#12).
                api.prevent_close();
                event_hud.dismiss_from_user(DismissReason::CloseRequested);
            }
            WindowEvent::Focused(focused) => event_hud.on_focus_changed(*focused),
            WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } => {
                // The window may have been dragged or the DPI changed: the supervisor
                // re-reads the geometry on the next tick, nothing to do here.
            }
            _ => {}
        });

        hud
    }

    pub fn phase(&self) -> Phase {
        self.motion
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .phase()
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// The webview finished its first render. This is the only place that maps the
    /// window: from now on visibility is pure movement.
    pub fn mark_ready(self: &Arc<Self>) {
        if self.ready.swap(true, Ordering::SeqCst) {
            return;
        }
        self.reveal_offscreen();

        let pending = PENDING_SUMMON.swap(false, Ordering::SeqCst);
        if self.launch_mode == LaunchMode::ShowHud || pending {
            self.summon_from_user(SummonReason::Launch);
        }
    }

    /// Global-hotkey / tray-left-click toggle.
    pub fn toggle_from_user(self: &Arc<Self>, trigger: UserTrigger) {
        if !self.is_ready() {
            PENDING_SUMMON.store(true, Ordering::SeqCst);
            return;
        }

        let phase = self.phase();
        let transition = self
            .motion
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .toggle(Instant::now());

        match transition {
            Some(Transition::Summon) => self.start_summon(trigger.summon_reason()),
            Some(Transition::Dismiss) => self.start_dismiss(trigger.dismiss_reason()),
            None if phase.is_on_screen() => self.focus_window(),
            None => {}
        }
    }

    /// Explicit summon (first launch, second instance, tray "设置…").
    pub fn summon_from_user(self: &Arc<Self>, reason: SummonReason) {
        if !self.is_ready() {
            PENDING_SUMMON.store(true, Ordering::SeqCst);
            return;
        }

        let phase = self.phase();
        let transition = self
            .motion
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .summon(Instant::now());

        if transition == Some(Transition::Summon) {
            self.start_summon(reason);
        } else if phase.is_on_screen() {
            // Already on screen: just bring it forward, never re-animate.
            self.focus_window();
        }
    }

    /// Explicit dismiss (Esc, close request).
    pub fn dismiss_from_user(self: &Arc<Self>, reason: DismissReason) {
        let transition = self
            .motion
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .dismiss(Instant::now());
        if transition == Some(Transition::Dismiss) {
            self.start_dismiss(reason);
        }
    }

    /// Tray "设置…": summon, then ask the webview to open the settings layer (T22).
    pub fn open_settings(self: &Arc<Self>) {
        self.summon_from_user(SummonReason::Tray);
        let _ = self.window.emit(events::OPEN_SETTINGS, ());
    }

    /// Tray "关于 / 检查更新": summon, then ask the webview to open the about page (T23).
    pub fn open_about(self: &Arc<Self>) {
        self.summon_from_user(SummonReason::Tray);
        let _ = self.window.emit(events::OPEN_ABOUT, ());
    }

    /// Window focus changes feed the blur grace window.
    pub fn on_focus_changed(&self, focused: bool) {
        let mut motion = self
            .motion
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if focused {
            motion.note_focus_gained();
        } else {
            motion.note_focus_lost(Instant::now());
        }
    }

    pub fn snapshot(&self) -> HudSnapshot {
        let hotkey = self
            .app
            .try_state::<HotkeyState>()
            .map(|state| state.status())
            .unwrap_or_else(|| HotkeyStatus::failed("", "hotkey state not initialised"));
        HudSnapshot {
            phase: self.phase().as_str(),
            launch_mode: match self.launch_mode {
                LaunchMode::ShowHud => "show",
                LaunchMode::Resident => "resident",
            },
            hotkey,
        }
    }

    /// Replaces the webview-reported hitboxes (CSS pixels, window relative).
    pub fn set_hit_boxes(&self, rects: Vec<geometry::LogicalRect>) {
        self.hitboxes.replace(rects);
    }

    /// One supervisor tick: blur grace, then click-through hit testing.
    /// Returns false once the window is gone so the supervisor thread can stop.
    pub fn tick(self: &Arc<Self>) -> bool {
        if !self.is_ready() {
            return true;
        }

        let due = {
            let mut motion = self
                .motion
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            motion.due_blur_dismiss(Instant::now())
        };
        if due == Some(Transition::Dismiss) {
            self.start_dismiss(DismissReason::Blur);
        }

        if self.phase().is_on_screen() {
            if self.poll_cursor().is_err() {
                return false;
            }
        } else {
            let mut ignore = self
                .ignore_cursor
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(value) = ignore.force_ignore() {
                let _ = self.window.set_ignore_cursor_events(value);
            }
        }
        true
    }

    /// Parks the window just below the monitor without showing it, then shows it.
    fn reveal_offscreen(&self) {
        let size = self.logical_size();
        let bounds = self.monitor_bounds();
        let target = geometry::summon_target(*self.lock(&self.last_visible), size, &bounds);
        let dock = geometry::dock_position(target, &bounds);

        let _ = self
            .window
            .set_position(LogicalPosition::new(dock.0, dock.1));
        let _ = self.window.set_ignore_cursor_events(true);
        if !self.window.is_visible().unwrap_or(false) {
            if let Err(error) = self.window.show() {
                eprintln!("[aetherhud] failed to map the HUD window: {error}");
            }
        }
        tray::sync(&self.app, self.phase());
    }

    fn start_summon(self: &Arc<Self>, reason: SummonReason) {
        self.remember_previous_app();

        let size = self.logical_size();
        let bounds = self.monitor_bounds();
        let target = geometry::summon_target(*self.lock(&self.last_visible), size, &bounds);
        let dock = geometry::dock_position(target, &bounds);

        let visible = self.window.is_visible().unwrap_or(false);
        let from = match self.current_position() {
            Some(position) if visible => geometry::clamp_to_monitor(position, size, &bounds),
            _ => dock,
        };

        // Windows foreground lock: show → set_position → set_focus in one tight
        // sequence, otherwise the first click after a summon is eaten (#24 §8).
        if !visible {
            let _ = self
                .window
                .set_position(LogicalPosition::new(from.0, from.1));
            if let Err(error) = self.window.show() {
                eprintln!("[aetherhud] failed to map the HUD window: {error}");
            }
        }
        let _ = self.window.set_ignore_cursor_events(true);
        self.focus_window();
        // Emit only after the window owns focus: the webview answers with a DOM focus
        // on the search box, which must not race the OS focus grant (#16 K1).
        let _ = self.window.emit(events::SUMMON, ());
        if cfg!(debug_assertions) {
            eprintln!("[aetherhud] summon ({reason:?}) from {from:?} to {target:?}");
        }
        self.animate(from, target, Transition::Summon);
        tray::sync(&self.app, self.phase());
    }

    fn start_dismiss(self: &Arc<Self>, reason: DismissReason) {
        if cfg!(debug_assertions) {
            eprintln!("[aetherhud] dismiss ({reason:?})");
        }
        let _ = self.window.emit(events::DISMISS, ());

        // 收起开始即归还焦点：hand focus back before the slide, so the previous
        // application can accept typing while the HUD is still leaving.
        if reason.restores_previous_app() {
            self.restore_previous_app();
        } else {
            // A blur dismissal means the user already moved on; drop the stale handle.
            let _ = self.lock(&self.previous_app).take();
        }
        #[cfg(target_os = "macos")]
        panel::resign_key(&self.app);

        let mut ignore = self
            .ignore_cursor
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(value) = ignore.force_ignore() {
            let _ = self.window.set_ignore_cursor_events(value);
        }
        drop(ignore);

        let size = self.logical_size();
        let current = self.current_position().unwrap_or((0.0, 0.0));
        let bounds = self.monitor_bounds_at(current, size);
        let on_screen = geometry::clamp_to_monitor(current, size, &bounds);
        *self.lock(&self.last_visible) = Some(on_screen);
        let dock = geometry::dock_position(on_screen, &bounds);

        self.animate(current, dock, Transition::Dismiss);
        tray::sync(&self.app, self.phase());
    }

    /// Slides the window between two logical positions. A new animation cancels the
    /// previous one through the generation counter, so fast toggles never fight.
    fn animate(self: &Arc<Self>, from: (f64, f64), to: (f64, f64), transition: Transition) {
        let generation = self.animation.fetch_add(1, Ordering::SeqCst) + 1;
        let window = self.window.clone();
        let hud = Arc::clone(self);
        let animation = Arc::clone(&self.animation);

        thread::spawn(move || {
            let started = Instant::now();
            let duration_ms = SLIDE_DURATION.as_millis() as f64;

            loop {
                if animation.load(Ordering::SeqCst) != generation {
                    return;
                }
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                let progress = geometry::progress(elapsed_ms, duration_ms);
                let eased = geometry::ease_out_cubic(progress);
                let (x, y) = geometry::interpolate(from, to, eased);
                if window.set_position(LogicalPosition::new(x, y)).is_err() {
                    break;
                }
                if progress >= 1.0 {
                    break;
                }
                thread::sleep(ANIMATION_TICK);
            }

            if animation.load(Ordering::SeqCst) != generation {
                return;
            }

            {
                let mut motion = hud
                    .motion
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                match transition {
                    Transition::Summon => motion.note_shown(),
                    Transition::Dismiss => motion.note_docked(),
                }
            }

            if transition == Transition::Dismiss {
                let _ = window.set_ignore_cursor_events(true);
            }
            tray::sync(&hud.app, hud.phase());
        });
    }

    fn focus_window(&self) {
        let _ = self.window.set_focus();
        #[cfg(target_os = "macos")]
        panel::make_key(&self.app);

        // Windows may reject the first focus request under the foreground lock; one
        // retry on the next event-loop turn is enough in practice.
        if !self.window.is_focused().unwrap_or(true) {
            let window = self.window.clone();
            let app = self.app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = window.set_focus();
            });
        }
    }

    fn remember_previous_app(&self) {
        let mut previous = self.lock(&self.previous_app);
        if previous.is_none() {
            *previous = focus::capture(&self.window);
        }
    }

    fn restore_previous_app(&self) {
        let previous = self.lock(&self.previous_app).take();
        if let Some(previous) = previous {
            if !focus::restore(previous) {
                eprintln!("[aetherhud] could not hand focus back to the previous application");
            }
        }
    }

    fn poll_cursor(&self) -> Result<(), tauri::Error> {
        let origin = self.window.outer_position()?;
        let scale = self.window.scale_factor()?;
        let Some(cursor) = platform::cursor_position(&self.app, scale) else {
            return Ok(());
        };
        let hit = self.hitboxes.hits((origin.x, origin.y), scale, cursor);

        let mut ignore = self
            .ignore_cursor
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(value) = ignore.update(hit) {
            let _ = self.window.set_ignore_cursor_events(value);
        }
        Ok(())
    }

    fn logical_size(&self) -> (f64, f64) {
        let scale = self.window.scale_factor().unwrap_or(1.0);
        match self.window.outer_size() {
            Ok(size) if size.width > 0 && size.height > 0 => {
                (size.width as f64 / scale, size.height as f64 / scale)
            }
            _ => (1080.0, 720.0),
        }
    }

    fn current_position(&self) -> Option<(f64, f64)> {
        let scale = self.window.scale_factor().ok()?;
        let position = self.window.outer_position().ok()?;
        Some(geometry::to_logical((position.x, position.y), scale))
    }

    fn available_monitors(&self) -> Vec<MonitorBounds> {
        self.window
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(monitor_bounds_of)
            .collect()
    }

    /// The monitor to summon on: the one holding the remembered position, else the
    /// window's current monitor, else the primary monitor (#24 §8: 每次唤起重查).
    fn monitor_bounds(&self) -> MonitorBounds {
        let monitors = self.available_monitors();
        let size = self.logical_size();
        if let Some(position) = *self.lock(&self.last_visible) {
            let center = (position.0 + size.0 / 2.0, position.1 + size.1 / 2.0);
            if let Some(bounds) =
                geometry::monitor_for_point(&monitors, center).and_then(|index| monitors.get(index))
            {
                return *bounds;
            }
        }
        if let Some(monitor) = self.window.current_monitor().ok().flatten() {
            return monitor_bounds_of(&monitor);
        }
        if let Some(monitor) = self.window.primary_monitor().ok().flatten() {
            return monitor_bounds_of(&monitor);
        }
        monitors
            .first()
            .copied()
            .unwrap_or_else(|| MonitorBounds::new(0.0, 0.0, 1920.0, 1080.0, 1.0))
    }

    fn monitor_bounds_at(&self, position: (f64, f64), size: (f64, f64)) -> MonitorBounds {
        let monitors = self.available_monitors();
        let center = (position.0 + size.0 / 2.0, position.1 + size.1 / 2.0);
        if let Some(bounds) =
            geometry::monitor_for_point(&monitors, center).and_then(|index| monitors.get(index))
        {
            return *bounds;
        }
        self.monitor_bounds()
    }

    /// Locks a field, recovering from a poisoned mutex (a panicking animation thread
    /// must not take the whole shell down).
    fn lock<'a, T>(&self, target: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
        target.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

fn monitor_bounds_of(monitor: &Monitor) -> MonitorBounds {
    let scale = monitor.scale_factor();
    let position = monitor.position();
    let size = monitor.size();
    MonitorBounds::new(
        position.x as f64 / scale,
        position.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
        scale,
    )
}

/// Remembers a summon request that arrived before the webview was ready.
pub fn request_pending_summon() {
    PENDING_SUMMON.store(true, Ordering::SeqCst);
}

/// macOS: converts the main window into an NSPanel (called from setup).
#[cfg(target_os = "macos")]
pub fn install_panel(hud: &Arc<HudWindow>) -> tauri::Result<()> {
    panel::install(hud.window.clone(), Arc::clone(hud))
}

/// Periodically ticks the HUD: blur grace and click-through hit testing.
pub fn spawn_supervisor(hud: Arc<HudWindow>) {
    thread::spawn(move || loop {
        thread::sleep(clickthrough::POLL_INTERVAL);
        if !hud.tick() {
            break;
        }
    });
}

/// Parks the window off screen if the webview never reports ready.
pub fn spawn_reveal_fallback(hud: Arc<HudWindow>) {
    thread::spawn(move || {
        thread::sleep(REVEAL_FALLBACK_DELAY);
        if !hud.is_ready() {
            eprintln!(
                "[aetherhud] webview never reported ready; parking the HUD off screen instead"
            );
            hud.reveal_offscreen();
        }
    });
}