# Product Requirements Document (PRD)

## 1. 产品名称与定位
- **产品名称**：AetherHUD（或 SAO Nexus）
- **定位**：面向重度信息囤积者的极速桌面唤出式多源信息中枢（跨平台 Windows / macOS）。
- **核心理念**：
  - **宽进**：随手扔进收件箱（支持本地输入框、群聊机器人、多维表格定时同步）。
  - **秒分**：Jina Reader / 元数据提取 + Jev 极速分类引擎，实现静默全自动归类。
  - **优雅查**：SAO Utils 风格悬浮透明 HUD，L1/L2 两级快速收敛，L3 标签点击即时正交过滤。

---

## 2. 功能架构与数据流图

```text
[ 输入源渠道 ]
 ├── 1. 桌面快捷唤出输入框 (Alt+Space)
 ├── 2. 聊天 Bot (飞书 / 钉钉 / 微信 / TG Webhook)
 └── 3. 云端表格定时轮询 (飞书多维表格 / 腾讯文档 / Google Sheets)
            │
            ▼
[ Ingestion & 预处理管线 (Rust) ]
 ├── 判定输入类型: (裸文本 / 网页 URL / 视频 URL)
 ├── 网页: 调用 Jina Reader API (r.jina.ai) 提取 Markdown 正文
 └── 视频: 抓取 Title + Description + 置顶评论文本
            │
            ▼
[ Jev 决策模型引擎 (~100ms) ]
 ├── Choice 1 -> 确定 L1 (01 内容 / 02 效率 / 03 技术 / 04 商业)
 ├── Choice 2 -> 确定 L2 (如: 图像与设计)
 └── Multi-Noul -> 判定 L3 标签 (形态标签: 工具/教程/提示词; 特征标签: Midjourney等)
            │
            ▼
[ 本地持久化 (SQLite + FTS5) ]
            │
            ▼
[ SAO Utils 交互面板 (Tauri v2 + Webview) ]
 ├── 全局热键唤出 / 离开即隐藏
 ├── L1 & L2 环形/级联展开
 └── L3 标签气泡栏点击过滤 + 结果流卡片即开即用
```

---

## 3. 详细功能需求

### 3.1 跨渠道收件箱摄取模块 (Ingestion Pipeline)
1. **本地快速输入**：
   - 快捷键唤出 HUD 顶部单行输入框，支持粘贴 URL 或纯文本，按 Enter 瞬间入库并开始后台处理。
2. **第三方 Webhook 接收器 (内置 Axum HTTP 服务)**：
   - 本地开辟随机/固定端口（如 `http://127.0.0.1:28888/webhook/incoming`）。
   - 配合公网穿透（如 Cloudflare Tunnel）或内网环境，可接收来自 Telegram Bot、飞书自建应用 Bot、钉钉机器人的 POST 消息。
3. **多维表格定时任务轮询器 (Cron Sync Engine)**：
   - 在设置中配置飞书 Bitable / 腾讯文档 / Google Sheets 的 API Key 与 Base/Table ID。
   - Rust 后台配置轻量定时任务（如每 5 分钟），增量拉取状态为 `待归档` 的记录，写入本地并回写处理状态至云端表格。

### 3.2 智能预处理与 Jev 分类管道
1. **网页抓取容错**：
   - 发起请求至 `https://r.jina.ai/{TARGET_URL}`。
   - 超时限制为 4 秒，若超时则仅降级使用网页的 HTML `<title>`。
2. **视频元数据提取**：
   - 针对 Bilibili / YouTube 链接，调用轻量 API 抓取 `标题`、`简介正文` 与 `首条热门/置顶评论`。
3. **Jev 决策分类**：
   - 拼接上下文（截断前 1500 字符以内最关键上下文）。
   - 派发至 Jev Choice：强制单选命中预设的 L1 及对应的 L2。
   - 提取形态类型：从 `[工具, 教程, 提示词, 工作流, 素材]` 中判定 1~2 项作为 L3 基础标签。
   - 关键字/语义提取：自动挂载 1~3 个特定领域标签（如 `#Midjourney`, `#Python`）。

### 3.3 SAO Utils 风格前端交互 (UI/UX)
1. **唤出与动画**：
   - 热键唤出（默认 macOS `Option+Space`，Windows `Alt+Space`）。
   - 窗口具有倾斜角度、发光粒子边框、缓动扇形展开动画。
2. **过滤逻辑**：
   - 选中 L1 -> 展开二级分支 L2。
   - 右侧上方列出当前 L2 下所有条目所包含的 **L3 标签 Capsule/Pill**。
   - **交互方式**：标签支持**单选/多选点亮**（Toggle）。点亮多个标签时执行 `AND` 过滤，迅速收窄结果列表。
3. **沉浸式退出**：
   - 点击外部区域、失焦（Blur）或按下 `ESC` 时，窗口执行 150ms 淡出动画并隐藏。

---

## 4. 非功能性需求
1. **内存与资源开销**：常驻后台内存不超过 50 MB，CPU 占用日常 0%。
2. **离线高可用**：在断网环境下，抓取和 AI 管道进入队列挂起状态，本地搜索与 HUD 浏览功能不受任何影响。
3. **跨平台兼容**：单一代码库，同时编译输出 macOS (.dmg / .app) 与 Windows (.msi / .exe)。
