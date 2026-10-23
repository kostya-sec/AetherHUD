# 设置页信息架构（Settings IA Spec v1）

> 状态：**已确认**（2026-10-23，随 #29 落地）
> 票：[#29 凭证与 Keychain + 设置弹层骨架](https://github.com/kostya-sec/AetherHUD/issues/29)
> 上位规格：[v1 实现规格 #24](https://github.com/kostya-sec/AetherHUD/issues/24) §8（设置弹层信息架构）、§11（设置、凭证与网络）
> 决策依据：#15（API 凭证与密钥存放，见 [api-keys.md](./api-keys.md)）、#10 §3.8（settings 表）、#17（Esc 退层与键盘模型）、#7（HUD 观感与弹窗透明度）
> 适用代码：`crates/aether-core/src/credentials.rs`、`crates/aether-core/src/storage.rs`（settings 访问器）、`src-tauri/src/runtime.rs`、`src-tauri/src/commands.rs`、`src/hud-model/{types,state,selectors,constants,keyboard}.ts`、`src/platform/settings.ts`、`src/components/HudSettingsModal.tsx`、`src/components/HudCredentialsPane.tsx`、`src/components/HudSettingsRow.tsx`

## 1. 入口与层级

设置弹层是 HUD 里与「快速录入」「收件箱」同级的浮层，有两种入口：

1. **设置球 → 入口行 → 「设置」**（主入口，键盘可达性由 #38 接线）：点击球列最后一颗设置球，原地展开全局入口行（收件箱 / 全部 / 标签管理 / 分类管理 / 封存箱 / 回收站 / 设置 / 关于）；点「设置」打开弹层。入口行在同一时刻只与 L2 面板、L3 条三选一显示，打开任一支时其余两支折叠。
2. **托盘「设置…」/「关于 / 检查更新」**：壳先唤出 HUD，再发 `hud://settings/open` / `hud://about/open`；前端分别打开「通用」与「关于」分区。托盘「关于」在本票只落到设置弹层的「关于」分区（分区内容占位，检查更新归 #47）。

Esc 的退层顺序（#17 §5.1，本票补入设置弹层）：

```text
快速录入弹窗 → 设置弹层 → 列表视图（收件箱/全部/…） → 展开的入口行 → 收起 HUD
```

- 设置弹层打开时 Esc 只关闭弹层，HUD 留在原处（不收起）；关闭后焦点归还规则仍归 #38。
- 弹层**不因点击背景关闭**，避免误丢正在输入的密钥；关闭只有 Esc 与标题栏 ×。

## 2. 弹层外壳

| 项 | 规格 |
| --- | --- |
| 尺寸/位置 | 全窗 backdrop + 居中面板，面板最大宽 4xl、占满可用高度；内容区独立滚动 |
| 结构 | 顶部标题行（标题 + ×）→ 左侧十区导航 / 右侧分区内容 → 底部安全说明与 Esc 提示 |
| 透明度 | **弹层面板透明度 = 全局 HUD 透明度 + 3%**（`modalSurfaceStyle`：`min(HUD_OPACITY + 0.03, 1)`；HUD_OPACITY 在 #46 前固定 0.65）。整窗 backdrop 是独立的暗化层（`bg-stone-950/60`），不参与"弹窗透明度"的定义 |
| 背景交互 | 弹层打开时底层 HUD `inert`；backdrop 标 `data-hud-hitbox`，点击落在 webview 内，不会穿透到桌面触发失焦收起 |
| 焦点 | 打开即把焦点移入弹层容器（`tabIndex=-1`）；Tab 顺序按 DOM：标题行 × → 左侧十区导航 → 内容区。十区导航的方向键/Home/End 与焦点归还归 #38，本票用原生 Tab |
| 分区切换 | 点击左侧分区立即切换右侧内容；十个分区中的未实现区显示占位（分区职责 + 后续票） |

## 3. 十区信息架构

左侧导航顺序即下表顺序（`SETTINGS_SECTIONS`）；`implemented` 标记本票未实现的分区显示占位。

| # | 分区 | v1 内容 | 本票状态 | 后续票 |
| --- | --- | --- | --- | --- |
| 1 | 通用 | 开机自启、在 Dock 中显示、显示时区、退出 AetherHUD | 占位 | #46 |
| 2 | 外观与音效 | 主题色 5 套、HUD 透明度、雨幕密度、音效开关/音量 | 占位 | #46 |
| 3 | 分类与分类器 | provider / base URL / model / prompt 版本（只读）/ 测试连接 | 占位（凭证在「凭证」区） | #45 |
| 4 | 网络 | 代理三档（跟随系统 / 自定义 / 直连）+ 地址/账号/密码 | 占位 | #45 |
| 5 | **凭证** | 百炼 Key、Jina Key、飞书 App ID / App Secret 的保存/清除/状态/验证 | **已实现** | 本票 #29 |
| 6 | 摄入与同步 | webhook 路径复制/重置、端口、飞书 Base/Table/字段映射/开关/最近状态 | 占位 | #42 / #44 |
| 7 | 标签管理 | 标签合并、别名、常驻与隐藏 | 占位 | #35 |
| 8 | 分类管理 | 自建节点停用/恢复/改名、覆盖层导出 | 占位 | #41 |
| 9 | 数据 | 最近备份、立即备份、打开数据文件夹、清空回收站 | 占位 | #43 |
| 10 | 关于 | 版本、检查更新、忽略版本、下载入口、安装前必读、许可 | 占位 | #47 |

底部「退出 AetherHUD」随本票接入（`quit_app` IPC → `app.exit(0)`），是托盘菜单之外唯一的退出路径（#24 §10）；「通用」分区的其余项仍归 #46。

## 4. 凭证区

### 4.1 凭证清单

| 项 | account（Keychain） | settings 引用键 | 验证方式 |
| --- | --- | --- | --- |
| 百炼 API Key | `classifier.api_key` | `classifier.api_key_ref` | `GET {base_url}/models`（Bearer） |
| Jina API Key | `jina.api_key` | `jina.api_key_ref` | `GET https://r.jina.ai/https://example.com`（Bearer，一次读） |
| 飞书 App ID | `feishu.app_id` | `feishu.app_id_ref` | 与 App Secret 一起换 `tenant_access_token` |
| 飞书 App Secret | `feishu.app_secret` | `feishu.app_secret_ref` | 同上 |

- Keychain service 固定 `aetherhud`（`KEYCHAIN_SERVICE`）；引用形态固定 `keychain://aetherhud/<account>`。
- YouTube Key 在凭证区显示一行**不可编辑的占位**（v1 不配置，视频元数据走 oEmbed）；代理密码（`proxy.password`）归「网络」分区、飞书 Base app_token 与字段映射归「摄入与同步」分区（后续票），凭证区只留说明。
- 分类器 base URL 默认 `https://dashscope.aliyuncs.com/compatible-mode/v1`；settings 键 `classifier.base_url` 可覆盖（#45 出 UI）。

### 4.2 状态

界面只显示三态，**永不显示密钥本体**：

| 状态 | 判定 | 界面 |
| --- | --- | --- |
| 已配置 | Keychain 能读到非空密钥 | 绿色「已配置」+ `keychain://aetherhud/<account>` 引用串 |
| 未配置 | Keychain 无该条目（或密钥为空） | 灰色「未配置」+「将存于 <引用>」（说明指针形态，不代表已写入） |
| 钥匙串不可用 | Keychain 读取报错（平台不支持/无权限） | 红色「钥匙串不可用」+「将存于 <引用>」 |

- 状态由 Keychain 实际内容派生；settings 里的引用只用于展示与记录，不是「已配置」的判定依据（半写状态不会骗过界面）。
- 凭证区每次打开弹层都重读一次状态。

### 4.3 保存 / 清除

- **保存**：去掉首尾空白 → 空值拒绝（`CredentialError::EmptySecret`，界面按钮本就禁用）→ 密钥写 Keychain → settings 写引用（`INSERT … ON CONFLICT`）。引用写失败会回滚 Keychain 写入，避免「有密钥没引用」的半写状态。
- **清除**：先删 settings 引用，再删 Keychain 条目；钥匙串删除失败时把引用写回，避免「有密钥没引用」的半写状态。未配置时幂等成功。
- 保存/清除成功后，该外部服务的旧验证结论立即作废（分类 Key → 百炼验证；Jina Key → Jina 验证；任一半飞书凭证 → 飞书验证），界面不展示过期结论。

### 4.4 验证连通性

- 百炼：`GET {base}/models` + `Authorization: Bearer <key>`；200 且能解析模型数 → 「百炼连接正常（N 个模型）」。
- Jina：`GET https://r.jina.ai/https://example.com` + Bearer；200 且正文非空 → 「Jina Reader 可用（一次读取成功）」。
- 飞书：`POST https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal`，JSON body 带 `app_id` / `app_secret`；200 且 `code == 0` 且有 token → 「飞书 tenant_access_token 获取成功（有效期 N 秒）」。缺任一半时直接提示「请先保存飞书 App ID 与 App Secret」，不发请求。
- 超时 20s（Jina 平均 7.9s，给足余量）；验证期间按钮显示「验证中…」且不阻塞其他分区浏览。
- 失败一律给可读原因并分类提示：
  - **代理/网络**：无法连接 + 「若本机需要代理，请在『网络』设置中配置代理后重试」（Jina 明确提示本机通常需要代理）；
  - **Key**：401/403 → 「Key 无效或没有访问权限」；Jina 402/403 → 「Key 无效或额度不足」；
  - **权限/配置**：飞书错误码（`code` + `msg`）→ 「请检查 App ID / App Secret 是否有效、应用是否已发布」；404 → 检查 Base URL；
  - **限流/服务端**：429 / 5xx 提示稍后重试。
- 验证结果返回前按本次用到的密钥做一次擦除（`scrub_secrets`，前 4 后 4、≤8 字符整体遮蔽），即使传输层错误串或上游 msg 回显密钥也不会泄漏。

### 4.5 无 Key 降级

| 缺什么 | 行为 |
| --- | --- |
| 分类 Key | 分类器不可用 → 条目落「00 未分类 / 待人工归档」（#32 接入时按其 fail-safe 语义）；凭证区显示未配置，验证给出提示而不是崩溃 |
| Jina Key | 抓取走匿名额度或纯 L1 直连（#33） |
| 飞书凭证 | 同步源不轮询/不显示（#44） |
| 钥匙串不可用 | 凭证区四行都显示「钥匙串不可用」，保存/清除返回可读错误；HUD 其他功能不受影响 |

`CredentialService::secret()` 未配置时返回 `Ok(None)`，只有 Keychain 故障才返回 `Err`；调用方按 `Option` 分支降级，不 panic。

## 5. 存储、安全与隐私

- settings 与数据库**只存引用**：`keychain://aetherhud/<account>`；API Key 本体只存在 OS Keychain（Windows 凭据管理器 / macOS 钥匙串）。
- 日志、错误串、IPC 返回值都不含密钥；验证失败串再按已知密钥擦除一次。`redact_secret` 为统一脱敏函数（前 4 后 4）。
- 设置页只显示「已配置/未配置/不可用」与引用串，不提供「查看密钥」。
- 输入框 `type=password`、不自动回填已存密钥（每次保存都是整值替换）。
- 开发期 `.local/keys.env` 仍不是运行时来源（api-keys.md §2.1）。

## 6. 前后端契约

| 方向 | 名称 | 语义 |
| --- | --- | --- |
| 前端 → Rust | `settings_credential_statuses` | 返回四行 `CredentialStatus`（kind/account/label/state/reference），顺序固定 |
| 前端 → Rust | `settings_save_credential`（kind, secret） | 写 Keychain + settings 引用，返回该项新状态；空值/钥匙串失败返回可读错误 |
| 前端 → Rust | `settings_clear_credential`（kind） | 删 Keychain 条目 + settings 引用，幂等，返回新状态 |
| 前端 → Rust | `settings_validate_credential`（target ∈ classifier / jina / feishu） | 返回 `{ ok, detail }`；detail 已脱敏，失败含代理/Key/权限提示 |
| 前端 → Rust | `quit_app` | 设置弹层底部「退出 AetherHUD」→ `app.exit(0)`；退出不等 worker 收尾（#24 §10） |
| Rust → 前端 | `hud://settings/open` | 托盘「设置…」：打开弹层「通用」分区 |
| Rust → 前端 | `hud://about/open` | 托盘「关于」：打开弹层「关于」分区（内容占位，#47 接入） |

序列化约定：Rust 枚举 `serde(rename_all = "camelCase")`（`classifierApiKey` 等）、状态 `snake_case`（`configured` / `not_configured` / `unavailable`）。前端 `CredentialStatus` 与 Rust 同形。

## 7. 验证

自动（本仓库可跑）：

- Rust（`cargo test -p aether-core`，假 Keychain / 假 HTTP / 内存库）：
  - 空库四行「未配置」，account 与引用契约；
  - 保存只落引用：Keychain 有密钥、settings 只有 `keychain://`、设置表全量快照不含密钥明文；
  - 已有密钥（无引用）显示「已配置」；清除后状态回「未配置」且幂等；
  - 钥匙串不可用：状态「不可用」、保存给可读错误、不落引用、错误串不含密钥；
  - 脱敏：前 4 后 4、≤8 字符整体遮蔽、错误串按已知密钥擦除；
  - 验证：百炼 200/401、自定义 base URL、Jina 200/无 Key、飞书成功/错误码/缺一半，断言请求方法、URL、Bearer/JSON body、错误可读且不含密钥。
  - 出口 HTTP 适配器扩展为 `HttpMethod` + body（v1 唯一的 POST 是飞书 token 交换），其余抓取仍走 GET。
- 前端（`npx vitest run`）：
  - 设置弹层默认关闭、入口行「设置」开启；十区顺序与占位状态；
  - 打开/切换/关闭、Esc 只关弹层、唤出重置、收起动画冻结、关闭后迟到响应被忽略；
  - 凭证状态/验证/忙碌/提示的 reducer 行为与过期验证作废；
  - IPC 适配：读取、保存（trim）、清除、验证成功/失败、退出、托盘事件打开分区；
  - Esc 全局分层（弹窗 → 设置 → 列表 → 入口行 → 收起）。

手工冒烟（Windows 真机，本机 WebView2 CDP）：设置球 → 入口行 → 设置打开；十区切换与占位；凭证区保存后显示「已配置」、清除后回「未配置」；验证失败给出可读原因；Esc 关闭回 HUD；托盘「设置…」直接打开弹层。

## 8. 后续票与 v1 不做

- 十区其余分区的内容分别归 #45（分类器/网络）、#46（通用/外观/首启）、#42 / #44（摄入与同步）、#35（标签管理）、#41（分类管理）、#43（数据）、#47（关于与更新）。退出已在设置底部接入（本票），#46 只补通用分区其余项。
- 设置页五区键盘模型（方向键/Home/End/焦点归还）归 #38；本票只用原生 Tab。
- 设置变更即时广播前端事件（主题/透明度/音效/时区）归 #46；本票只做凭证与弹层骨架。
- 凭证的其余 account（`youtube.api_key` 占位、`proxy.password`、`feishu.app_token`）留到 #45 / #44；本票枚举只含四个已落地凭证，YouTube 只显示占位行。
- 不做密钥查看/导出/明文复制；不做设置项导入导出。