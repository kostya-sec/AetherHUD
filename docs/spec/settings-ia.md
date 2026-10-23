# 设置页信息架构（Settings IA Spec v1）

> 状态：**已确认**（2026-10-23 随 #29 落地；同日补入 #45 前置的 LLM 供应商配置）
> 票：[#29 凭证与 Keychain + 设置弹层骨架](https://github.com/kostya-sec/AetherHUD/issues/29)
> 上位规格：[v1 实现规格 #24](https://github.com/kostya-sec/AetherHUD/issues/24) §8（设置弹层信息架构）、§11（设置、凭证与网络）
> 决策依据：#15（API 凭证与密钥存放，见 [api-keys.md](./api-keys.md)）、#10 §3.8（settings 表）、#17（Esc 退层与键盘模型）、#7（HUD 观感与弹窗透明度）
> 适用代码：`crates/aether-core/src/credentials.rs`、`crates/aether-core/src/provider.rs`（LLM 供应商）、`crates/aether-core/src/storage.rs`（settings 访问器）、`src-tauri/src/runtime.rs`、`src-tauri/src/commands.rs`、`src/hud-model/{types,state,selectors,constants,keyboard}.ts`、`src/platform/settings.ts`、`src/components/HudSettingsModal.tsx`、`src/components/HudCredentialsPane.tsx`、`src/components/HudSettingsRow.tsx`

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
| 5 | **凭证** | 分类器供应商卡片（预设 / 消息类型 / Base URL / 模型 / API Key 与验证同卡片）+ Jina Key、飞书 App ID / App Secret 的保存/清除/状态/验证 | **已实现** | #29 / #45 |
| 6 | 摄入与同步 | webhook 路径复制/重置、端口、飞书 Base/Table/字段映射/开关/最近状态 | 占位 | #42 / #44 |
| 7 | 标签管理 | 标签合并、别名、常驻与隐藏 | 占位 | #35 |
| 8 | 分类管理 | 自建节点停用/恢复/改名、覆盖层导出 | 占位 | #41 |
| 9 | 数据 | 最近备份、立即备份、打开数据文件夹、清空回收站 | 占位 | #43 |
| 10 | 关于 | 版本、检查更新、忽略版本、下载入口、安装前必读、许可 | 占位 | #47 |

底部「退出 AetherHUD」随本票接入（`quit_app` IPC → `app.exit(0)`），是托盘菜单之外唯一的退出路径（#24 §10）；「通用」分区的其余项仍归 #46。

## 4. LLM 供应商与凭证

### 4.1 供应商配置（非秘密 settings）

凭证区第一张卡片是「分类器供应商」。按用户 2026-10-23 澄清，v1 采用**单一活动供应商 + 预设**：同一时间只有一套配置生效，不做多套配置列表；切换预设只填默认值，Key 仍共用 `classifier.api_key`，换供应商要重填 Key。

凭证区从上到下依次是：**分类器供应商卡片**（预设 / 消息类型 / Base URL / 模型 / API Key / 保存与验证同卡片）→ Jina Reader 分组 → 飞书开放平台分组 → YouTube 占位行与说明。分类器 API Key 不再单独成组（#45 修复：用户反馈供应商卡片里找不到填 Key 的地方），页面上只有一行 `classifierApiKey`。

| 预设（serde 值） | 默认 Base URL | 默认模型 | 默认消息类型 |
| --- | --- | --- | --- |
| 百炼（`bailian`，默认） | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-flash` | `chat_completions` |
| OpenAI（`openai`） | `https://api.openai.com/v1` | `gpt-4o-mini` | `chat_completions` |
| DeepSeek（`deepseek`） | `https://api.deepseek.com/v1` | `deepseek-chat` | `chat_completions` |
| 自定义（`custom`） | 空（必填） | 空（必填） | `chat_completions` |

- **消息类型（message type）**，二选一（决定请求体与响应解析）：
  - `chat_completions`（默认）：`POST {base}/chat/completions`，请求体 `messages[]`，读 `choices[0].message.content`；
  - `responses`：`POST {base}/responses`，请求体 `input[]`（system 也可作为首项），读顶层 `output_text` 或 `output[].content[].output_text`；usage 取 `input_tokens` / `output_tokens`。
- settings 键（都是 value_json 字符串，**不是秘密**）：
  - `classifier.provider`：预设值（`bailian` / `openai` / `deepseek` / `custom`）；
  - `classifier.base_url`：活动 Base URL；
  - `classifier.model`：模型 ID；
  - `classifier.message_type`：`chat_completions` / `responses`。
- Key 仍只存 Keychain account `classifier.api_key`，数据库只留 `classifier.api_key_ref`。改成任何供应商都不新增 Keychain 条目。
- **保存**：去首尾空白 → Base URL 去掉末尾 `/` → 校验 Base URL 非空、以 http/https 开头且带主机（长度 ≤2048），模型非空（≤256）→ 逐键写 settings → 回读并刷新草稿。校验失败返回可读错误，界面按钮不静默失败。
- **坏值回落**：settings 缺失或值无法解析（旧版本 / 手工改坏 / 非 HTTP 端点）时逐键回落到预设默认值；设置页永远能打开。自定义预设没填 Base URL 时按「未配置」处理。
- **接缝**：worker / eval 通过 `aether-core::provider::ProviderProfile::to_classifier_config(api_key)` 拿到 `aether-classifier` 的 `ProviderConfig`（temperature 0、max_tokens 400、30s、proxy None、extra_body 空，代理归「网络」分区）；分类 Key 的读取仍走 `CredentialService::secret()`。

### 4.2 凭证清单

| 项 | account（Keychain） | settings 引用键 | 验证方式 |
| --- | --- | --- | --- |
| 分类器 API Key（展示名随活动供应商） | `classifier.api_key` | `classifier.api_key_ref` | `GET {活动 Base URL}/models`（Bearer） |
| Jina API Key | `jina.api_key` | `jina.api_key_ref` | `GET https://r.jina.ai/https://example.com`（Bearer，一次读） |
| 飞书 App ID | `feishu.app_id` | `feishu.app_id_ref` | 与 App Secret 一起换 `tenant_access_token` |
| 飞书 App Secret | `feishu.app_secret` | `feishu.app_secret_ref` | 同上 |

- Keychain service 固定 `aetherhud`（`KEYCHAIN_SERVICE`）；引用形态固定 `keychain://aetherhud/<account>`。
- YouTube Key 在凭证区显示一行**不可编辑的占位**（v1 不配置，视频元数据走 oEmbed）；代理密码（`proxy.password`）归「网络」分区、飞书 Base app_token 与字段映射归「摄入与同步」分区（后续票），凭证区只留说明。
- 分类器 API Key 是「分类器供应商」卡片内的固定一行：密码输入 + 三态徽标 + `keychain://` 引用 + 「保存」/「清除」。保存 Key 不改供应商配置，保存供应商配置也不清除 Key。
- 分类器 Key 行的展示名随**已保存**的活动供应商变化（阿里百炼（DashScope） / OpenAI / DeepSeek / 自定义（OpenAI 兼容）），account 与引用不变；只改了草稿未保存时，展示名仍跟随已保存供应商。

### 4.3 状态

界面只显示三态，**永不显示密钥本体**：

| 状态 | 判定 | 界面 |
| --- | --- | --- |
| 已配置 | Keychain 能读到非空密钥 | 绿色「已配置」+ `keychain://aetherhud/<account>` 引用串 |
| 未配置 | Keychain 无该条目（或密钥为空） | 灰色「未配置」+「将存于 <引用>」（说明指针形态，不代表已写入） |
| 钥匙串不可用 | Keychain 读取报错（平台不支持/无权限） | 红色「钥匙串不可用」+「将存于 <引用>」 |

- 状态由 Keychain 实际内容派生；settings 里的引用只用于展示与记录，不是「已配置」的判定依据（半写状态不会骗过界面）。
- 凭证区每次打开弹层都重读一次状态，同时读取活动供应商配置。

### 4.4 保存 / 清除

- **保存**：去掉首尾空白 → 空值拒绝（`CredentialError::EmptySecret`，界面按钮本就禁用）→ 密钥写 Keychain → settings 写引用（`INSERT … ON CONFLICT`）。引用写失败会回滚 Keychain 写入，避免「有密钥没引用」的半写状态。
- **清除**：先删 settings 引用，再删 Keychain 条目；钥匙串删除失败时把引用写回，避免「有密钥没引用」的半写状态。未配置时幂等成功。
- 保存/清除成功后，该外部服务的旧验证结论立即作废（分类 Key → 分类器验证；Jina Key → Jina 验证；任一半飞书凭证 → 飞书验证），界面不展示过期结论。
- **分类器 Key 行**：输入后回车与点「保存」等价；保存成功清空输入框（供应商配置草稿不清空）。未配置时「清除」禁用；「钥匙串不可用」时仍可点「保存」，由核心库返回可读错误。
- 「保存供应商配置」只写四个 `classifier.*` 非秘密 settings，不写 Keychain；保存成功后草稿与已保存配置一致，验证按钮才可用。保存供应商配置不清空草稿（配置本身要保留）；它也不清除 Key，切换预设后旧 Key 仍会用于验证，失败时提示重填。

### 4.5 验证连通性

- 分类器：`GET {活动 Base URL}/models` + `Authorization: Bearer <key>`；200 且能解析模型数 →「「百炼」连接正常（N 个模型，模型 qwen-flash）」。失败信息带上供应商名、活动 Base URL 与模型；自定义预设 Base URL 为空时直接提示「请先在凭证区保存供应商配置（Base URL 与模型），再验证分类器 Key」，不发请求。
- 分类器的「验证连接」按钮位于供应商卡片底部，只使用**已保存**的配置与钥匙串里的 Key：配置未保存（profile 为 null，或已保存的 Base URL / 模型为空）、Key 未配置、草稿有未保存改动时按钮禁用，并在卡片内给出可读原因（「先保存供应商配置…」「保存 API Key 后即可验证连接」）。验证结论显示在同一卡片内，不发请求就不会显示成功。
- Jina：`GET https://r.jina.ai/https://example.com` + Bearer；200 且正文非空 →「Jina Reader 可用（一次读取成功）」。
- 飞书：`POST https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal`，JSON body 带 `app_id` / `app_secret`；200 且 `code == 0` 且有 token →「飞书 tenant_access_token 获取成功（有效期 N 秒）」。缺任一半时直接提示「请先保存飞书 App ID 与 App Secret」，不发请求。
- 超时 20s（Jina 平均 7.9s，给足余量）；验证期间按钮显示「验证中…」且不阻塞其他分区浏览。
- 失败一律给可读原因并分类提示：
  - **代理/网络**：无法连接 +「若本机需要代理，请在『网络』设置中配置代理后重试」（Jina 明确提示本机通常需要代理）；
  - **Key**：401/403 →「Key 无效或没有访问权限」；Jina 402/403 →「Key 无效或额度不足」；
  - **权限/配置**：飞书错误码（`code` + `msg`）→「请检查 App ID / App Secret 是否有效、应用是否已发布」；分类器 404 → 检查 Base URL 与模型；
  - **限流/服务端**：429 / 5xx 提示稍后重试。
- 验证结果返回前按本次用到的密钥做一次擦除（`scrub_secrets`，前 4 后 4、≤8 字符整体遮蔽），即使传输层错误串或上游 msg 回显密钥也不会泄漏。

### 4.6 无 Key 降级

| 缺什么 | 行为 |
| --- | --- |
| 分类 Key | 分类器不可用 → 条目落「00 未分类 / 待人工归档」（#32 接入时按其 fail-safe 语义）；凭证区显示未配置，验证给出提示而不是崩溃 |
| Jina Key | 抓取走匿名额度或纯 L1 直连（#33） |
| 飞书凭证 | 同步源不轮询/不显示（#44） |
| 钥匙串不可用 | 凭证区四行都显示「钥匙串不可用」，保存/清除返回可读错误；HUD 其他功能不受影响 |
| 供应商配置缺失 | 按百炼默认值运行（自定义空 Base URL 除外）；设置页打开时自动加载，坏值回落不报错 |

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
| 前端 → Rust | `settings_provider_profile` | 返回活动 `ProviderProfile`（preset / baseUrl / model / messageType）；缺失或坏值已回落默认 |
| 前端 → Rust | `settings_save_provider_profile`（profile） | 校验并保存非秘密 settings（`classifier.provider` / `classifier.base_url` / `classifier.model` / `classifier.message_type`），返回规范化后的配置；校验失败返回可读错误 |
| 前端 → Rust | `settings_credential_statuses` | 返回四行 `CredentialStatus`（kind/account/label/state/reference），顺序固定 |
| 前端 → Rust | `settings_save_credential`（kind, secret） | 写 Keychain + settings 引用，返回该项新状态；空值/钥匙串失败返回可读错误 |
| 前端 → Rust | `settings_clear_credential`（kind） | 删 Keychain 条目 + settings 引用，幂等，返回新状态 |
| 前端 → Rust | `settings_validate_credential`（target ∈ classifier / jina / feishu） | 返回 `{ ok, detail }`；detail 已脱敏，失败含代理/Key/权限提示 |
| 前端 → Rust | `quit_app` | 设置弹层底部「退出 AetherHUD」→ `app.exit(0)`；退出不等 worker 收尾（#24 §10） |
| Rust → 前端 | `hud://settings/open` | 托盘「设置…」：打开弹层「通用」分区 |
| Rust → 前端 | `hud://about/open` | 托盘「关于」：打开弹层「关于」分区（内容占位，#47 接入） |

序列化约定：Rust 枚举 `serde(rename_all = "camelCase")`（`classifierApiKey` 等）、状态 `snake_case`（`configured` / `not_configured` / `unavailable`）。前端 `CredentialStatus` 与 Rust 同形。`ProviderProfile` 用 camelCase（`baseUrl` / `messageType`），`preset` 与 `messageType` 的枚举字面量是 snake_case（`bailian` / `chat_completions`）。

## 7. 验证

自动（本仓库可跑）：

- Rust（`cargo test -p aether-core`，假 Keychain / 假 HTTP / 内存库）：
  - 空库四行「未配置」，account 与引用契约；
  - 保存只落引用：Keychain 有密钥、settings 只有 `keychain://`、设置表全量快照不含密钥明文；
  - 已有密钥（无引用）显示「已配置」；清除后状态回「未配置」且幂等；
  - 钥匙串不可用：状态「不可用」、保存给可读错误、不落引用、错误串不含密钥；
  - 脱敏：前 4 后 4、≤8 字符整体遮蔽、错误串按已知密钥擦除；
  - 验证：百炼 200/401、自定义 base URL、Jina 200/无 Key、飞书成功/错误码/缺一半，断言请求方法、URL、Bearer/JSON body、错误可读且不含密钥。
  - 供应商配置（`aether-core::provider`）：预设默认值、默认等于百炼、非法 base URL / 空模型的可读错误、save/load 往返（含自定义与尾斜杠规范化）、坏值逐键回落、映射到 `aether-classifier` 的 `ProviderConfig`；分类器验证使用已保存的自定义 profile 的 Base URL，未配置 Base URL 时不发请求。
  - 两种消息类型（`aether-classifier::provider`）：chat / responses 的 endpoint 与请求体结构、chat 完整 URL 原样使用、responses 顶层 `output_text` 与 `output[]` 两种解析、usage 映射、缺失文本的可读终态错误。
  - 出口 HTTP 适配器扩展为 `HttpMethod` + body（v1 唯一的 POST 是飞书 token 交换），其余抓取仍走 GET。
- 前端（`npx vitest run`）：
  - 设置弹层默认关闭、入口行「设置」开启；十区顺序与占位状态；
  - 打开/切换/关闭、Esc 只关弹层、唤出重置、收起动画冻结、关闭后迟到响应被忽略；
  - 凭证状态/验证/忙碌/提示的 reducer 行为与过期验证作废；
  - IPC 适配：读取、保存（trim）、清除、验证成功/失败、退出、托盘事件打开分区；
  - 供应商配置：预设顺序与默认值、reducer 只在弹层打开时写入（关闭后迟到响应忽略）、打开/唤出时的重置、读取失败降级、前端规范化与校验（空 Base URL / 非 http(s) / 空模型）、保存失败保留草稿；
  - Esc 全局分层（弹窗 → 设置 → 列表 → 入口行 → 收起）。

手工冒烟（Windows 真机，本机 WebView2 CDP）：设置球 → 入口行 → 设置打开；十区切换与占位；凭证区「分类器供应商」卡片内输入假 Key（`data-provider-card` 下的 `classifierApiKey` 行）→ 保存后显示「已配置」、清除后回「未配置」；切 DeepSeek 预设 → Base URL / 模型自动填 → 保存配置后 settings 出现四个 `classifier.*` 键；未配置 Key / 草稿未保存时验证按钮禁用并给出提示（真机不发起真实网络验证）；Esc 关闭回 HUD；托盘「设置…」直接打开弹层。

## 8. 后续票与 v1 不做

- 十区其余分区的内容分别归 #45（分类器/网络）、#46（通用/外观/首启）、#42 / #44（摄入与同步）、#35（标签管理）、#41（分类管理）、#43（数据）、#47（关于与更新）。退出已在设置底部接入（本票），#46 只补通用分区其余项。
- **#45 前置已落地**：「分类与分类器」区的 provider / base URL / model / 消息类型与「测试连接使用活动供应商」已在「凭证」区实现；#45 只剩该分区的只读展示、prompt 版本、测试连接入口迁移与网络代理。
- 设置页五区键盘模型（方向键/Home/End/焦点归还）归 #38；本票只用原生 Tab。
- 设置变更即时广播前端事件（主题/透明度/音效/时区）归 #46；本票只做凭证与弹层骨架。
- 凭证的其余 account（`youtube.api_key` 占位、`proxy.password`、`feishu.app_token`）留到 #45 / #44；本票枚举只含四个已落地凭证，YouTube 只显示占位行。
- 不做密钥查看/导出/明文复制；不做设置项导入导出。