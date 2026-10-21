# 分发、更新与签名策略（Distribution & Update Spec v1）

> 状态：已确认（2026-10-21）
> 来源票：[决策：自动更新机制与安装包签名策略](https://github.com/kostya-sec/AetherHUD/issues/13)
> 依赖决议：#4（Tauri v2 透明窗口与安装形态）、#10（schema、meta 表与迁移备份）、#11（失败静默/重试语义）、#12（托盘与「不做系统通知」）、#17（默认态无卡片流、提示不得抢焦点）
> 一句话：**v1 不自动更新、不买证书**。应用内只做「查版本 → 提示 → 跳下载」，安装包走 GitHub Release + 固定网盘双通道；升级前的数据快照由新版本在迁移前自己完成。

## 1. 决策摘要

| 项 | v1 结论 |
|---|---|
| 自动更新（下载/安装/重启） | ❌ 不做；不引入 `tauri-plugin-updater`、不生成 minisign 密钥 |
| 应用内更新检查 | ✅ 做：只查版本并提示，不下载、不安装 |
| 检查频率 | 启动 30s 后一次 + 之后每 72h 一次 + 托盘/关于手动 |
| 版本信息源 | GitHub Releases API（单一可替换常量） |
| 下载通道 | GitHub Release 页 + 固定网盘分享文件夹（链接常量） |
| Windows 签名 | ❌ 不买 OV/EV；只发 NSIS `.exe`（当前用户安装、免管理员） |
| macOS 签名 | ❌ 不买 Apple Developer（$99/年）、不公证；ad-hoc 签名，只发 universal `.dmg` |
| 升级数据保护 | 新版本首次启动、迁移前 `VACUUM INTO` 备份（保留 5 份）+ 迁移失败回滚 |
| 版本号/通道 | SemVer；v1 只发正式版；预发布不上 |
| 系统通知 | ❌ 不发（#12 已否决） |

## 2. 更新检查

### 2.1 触发与节流

- 应用启动 **30 秒后**查一次（等首屏、摄取与 HUD 起来，不抢启动期网络）。
- 进程常驻期间每 **72 小时**查一次。
- 托盘右键「检查更新」、设置→关于「检查更新」按钮：随时手动，**不受 72h 节流**。
- 节流锚点落 `meta.last_update_check_at`（Unix ms；手动检查也写）。重启不会重复查。
- 只在正式版启用；debug/dev 构建跳过（或配置 `update.check_enabled = false` 强制关）。

### 2.2 数据源

- 默认端点：`https://api.github.com/repos/kostya-sec/AetherHUD/releases/latest`（未认证 60 次/小时/IP，本用例可忽略）。
- 端点 URL 是**单一配置常量**（Rust `const` 或配置项）：换镜像/换源只改这一处，不改检查逻辑。
- 超时 10s；失败不重试（等下一次节流点）。请求不带任何用户数据（无遥测）。

### 2.3 判定

- 解析 `tag_name` 为 SemVer，与当前版本比较；只有**大于**当前版本才算有更新。
- `prerelease = true` 或版本含预发布后缀 → 忽略。
- 取用字段：版本号、发布时间、`body`（纯文本，截断 500 字符）、`html_url`。

### 2.4 失败

- 自动检查失败（断网/被墙/限流/超时）：**完全静默**，不弹任何提示。
- 手动检查失败：提示「检查更新失败，可手动下载」+ 两个固定链接（GitHub Releases 页、网盘文件夹），不阻塞操作。
- 手动检查无更新：「已是最新版本 vX.Y.Z」。

## 3. 提示与交互

| 触发面 | 形态 | 交互 | 持久性 |
|---|---|---|---|
| 托盘菜单 | 菜单顶部一项「有新版本 vX.Y.Z…」；tooltip 追加「可更新 vX.Y.Z」 | 点击 = 唤出 HUD 并打开更新详情 | 直到更新/忽略/没有新版本 |
| 设置球展开条 | 「关于」项显示红点（与 #9 已定的「标签管理」同层级入口） | 打开关于页：当前版本、最新版本、检查按钮、两个下载链接、打开数据文件夹 | 同上 |
| HUD 内提示条 | 首次发现某版本后，下次唤出时显示 **5 秒**不阻塞提示条 | 点击进详情；右侧 × 关闭；不占焦点、不进入卡片流（#17） | 一次性，每个版本一次 |

- **不做系统通知、不做托盘数字角标**（#12）。
- 更新详情（HUD 内弹层，复用现有弹窗层级）：版本号、发布日期、notes 纯文本（可滚动）、按钮「GitHub 下载」「网盘下载」「忽略此版本」。
- 「忽略此版本」写 `meta.ignored_update_version`：之后自动检查不再提示该版本，直到出现更高版本；**手动检查仍显示**（用户主动问，必须如实回答）。
- 提示消失条件：点击任一下载按钮、忽略、或检查结果为已是最新。

## 4. 分发产物与版本

| 项 | 规则 |
|---|---|
| Windows | 仅 NSIS `.exe`，`installMode = currentUser`（免管理员、更新不需要 UAC）；命名 `AetherHUD_<ver>_x64-setup.exe`；不发 MSI、不发便携 zip |
| macOS | 仅 universal `.dmg`（arm64 + x86_64 一个包）；命名 `AetherHUD_<ver>_universal.dmg`；ad-hoc 签名、不公证 |
| 校验 | 每个 Release 附 `SHA256SUMS.txt`（列出全部产物） |
| 说明 | 每个 Release 附 `安装说明.txt`（与 README、Release 正文同一模板，见 §5.2） |
| Release 正文 | 顶部固定「安装前必读」→ 本版变更 → 校验值 → 两个下载链接 |
| 网盘 | 固定分享文件夹（百度/阿里云盘均可），每版上传同样的 exe/dmg/SHA256SUMS/安装说明；**文件夹链接是常量，不随版本变** |
| 版本号 | SemVer（MAJOR.MINOR.PATCH）；v1 只发正式版；修复走 PATCH；预发布用 GitHub prerelease 但不对外 |
| 发布步骤 | 构建 → 同机冒烟（Windows 装/卸、macOS 首开过门禁）→ 上传 GitHub Release（草稿→发布）→ 上传网盘 → 核对两处 SHA256 一致 |

## 5. 签名现状与文案

### 5.1 用户实际会遇到什么

| 平台 | v1 状态 | 首次安装 | 每次手动升级 |
|---|---|---|---|
| Windows | 未签名（无证书） | SmartScreen「Windows 已保护你的电脑」→ 更多信息 → 仍要运行；发布者显示「未知发布者」；企业策略 / Smart App Control 可能直接拦截 | 同样一次（浏览器下载带 MOTW） |
| macOS | ad-hoc 签名、未公证 | 双击被拦（「Apple 无法验证…」）→ 系统设置 → 隐私与安全性 → 仍要打开（按钮只在被拦后约 1 小时出现，过期需重新双击触发）→ 输入密码 | 每份新下载的副本都要再来一次（门禁例外只认那一个副本） |

- macOS 若提示「已损坏」：多半是下载不完整或被改动 → 重新下载并核对 SHA256，不要点「移到废纸篓」。
- 不采购证书的依据与重新评估的触发条件见 §7。

### 5.2 「安装前必读」模板（同一份复用三处）

- 落点：README 安装区 / 每个 GitHub Release 正文顶部 / 网盘 `安装说明.txt`。
- 内容顺序：
  1. 两个下载来源（GitHub / 网盘）与何时用哪个；
  2. Windows 步骤（更多信息 → 仍要运行）；
  3. macOS 步骤（含 1 小时窗口、过期要重新双击、「已损坏」处理）；
  4. SHA256 校验命令（PowerShell `Get-FileHash <file> -Algorithm SHA256`；macOS `shasum -a 256 <file>`）；
  5. 升级前会自动备份数据库、回退旧版怎么做（§6）。
- 应用内不放大段说明；设置→关于只留一行「安装/更新遇到问题？」→ 打开 README。

## 6. 升级与数据安全

- 新版本首次启动：`rusqlite_migration` 检测到 schema 需要升级时，先 `VACUUM INTO 'backups/aether-<ts>.db'`，保留最近 **5 份**（沿用 #10 §11 的既有策略，不另立 3 份规则）。
- 迁移失败：事务回滚，提示「数据未改动，已保留备份」，并显示备份路径。
- 设置→关于：「打开数据文件夹」按钮（打开 `app_data_dir`），方便手动备份。
- 回退旧版：装旧安装包 + 从 `backups/` 恢复对应时间的快照（旧版不认识新 schema，不能直接读新库）。Release 说明固定写这一句。

## 7. 为什么 v1 不买证书 / 不做自动更新

- **Windows**：OV/EV 都换不到「首装不弹」；EV 自 2024 起不再免检；Azure Artifact Signing 个人仅美/加开放，国内个人不可用；OV $150–300/年 + 强制硬件令牌。小圈子自用不划算。
- **macOS**：$99/年即可公证、体验最好，但 v1 是手动更新、频率低；不买不阻塞自用与朋友试用。
- **自动更新（A 方案）**：需要 minisign 私钥管理 + 发布流水线 + 更新失败兜底；macOS 无证书时是 ad-hoc 更新（2.14.0 起还有下载字节校验的坑），国内 GitHub 网络也不稳。收益（省一次手动安装）不抵成本，v1 只做「检查 + 提示」。
- **触发重新评估的条件（任一）**：公开发布/下载量上来；有 Mac 用户装不上或反复求助；作者自己嫌每次更新过门禁烦；出现需要快速推送修复的安全/数据事故。届时另开票处理，不属于本 PRD v2 范围。

## 8. 验收清单

- [ ] 启动 30s 后只查一次；重启不重复；72h 节流生效（改 `meta.last_update_check_at` 可验证）。
- [ ] 断网/限流/被墙时自动检查完全静默；手动检查给出双链接。
- [ ] prerelease 与不更新的版本不触发提示；更高版本正常触发。
- [ ] 忽略某版本后自动检查不再提示，手动检查仍显示。
- [ ] 首次发现某版本只弹一次 5s 提示条；不抢焦点、不影响键盘导航（#17）。
- [ ] Windows exe 在干净账户安装免管理员；macOS universal dmg 在 Apple Silicon 与 Intel 上都能首次打开过门禁。
- [ ] 每个 Release 资产齐全（exe / dmg / SHA256SUMS / 安装说明），正文含安装前必读与双链接。
- [ ] 旧版→新版首次启动生成 `backups/` 快照；迁移失败可回滚且提示明确。
- [ ] 设置→关于显示当前版本、可检查更新、打开数据文件夹；全程无系统通知（#12）。

## 9. 非目标（v1）

- 自动下载/安装/重启（`tauri-plugin-updater`、minisign、增量更新）。
- 系统通知、托盘数字角标（#12 已否决）。
- Windows MSI、便携 zip、ARM64 Windows；macOS Beta 通道 / App Store 分发。
- 代码签名证书采购与公证流水线（触发条件见 §7）。
- 更新文案本地化（v1 中文）、更新日志分级展示、灰度发布。

## 10. 参考

- [Tauri Updater 文档](https://v2.tauri.app/plugin/updater/)：自动更新能力与签名要求（v1 不采用）。
- [Microsoft：SmartScreen 声誉](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)：OV/EV 均先警告、EV 自 2024 不再免检、未签名每个版本从零攒声誉。
- [Microsoft：Windows 代码签名选项](https://learn.microsoft.com/zh-cn/windows/apps/package-and-deploy/code-signing-options)：Azure Artifact Signing 个人仅美/加；OV $150–300/年；EV $400+/年。
- [Tauri：macOS 签名](https://v2.tauri.app/distribute/sign/macos/)：$99/年 Apple Developer；未公证仍会显示未验证。
- [macOS 26 未签名应用打开流程](https://pcglance.com/no-open-anyway-button-macos-tahoe-26/)：Sequoia 起取消右键打开覆盖；「仍要打开」按钮约 1 小时时限。
