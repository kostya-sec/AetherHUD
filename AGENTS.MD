# AGENTS.md — Development Blueprint for AI Coding Agents

## 1. Project Context & Vision
- **Project Name**: `AetherHUD` (SAO-Style Resource Nexus).
- **Core Goal**: A ultra-lightweight, cross-platform (Windows & macOS) desktop floating HUD application built with **Rust + Tauri v2**.
- **UX Paradigm**: SAO Utils-inspired radial/cascading floating interface.
  - Hidden by default. Summoned via Global Hotkey. Dismissed on `ESC` or click outside.
  - Multi-source ingestion (Cloud Docs polling, Chatbot Webhooks, Desktop Quick Input).
  - 2+1 Tier Taxonomy: L1/L2 rigid hierarchy + L3 dynamic clickable Tag Pills.

## 2. Tech Stack & Architecture
- **Framework**: Tauri v2 (`core` + `plugins`).
- **Core Backend**: Rust (latest).
  - `rusqlite` (with FTS5 full-text search).
  - `tokio` (Async runtime for schedulers & webhooks).
  - `reqwest` + `serde` + `serde_json`.
  - `axum` (Embedded micro-server on localhost to receive incoming Bot Webhooks).
- **Frontend / HUD Layer**: Svelte 5 / Vue 3 + Tailwind CSS + Framer Motion (or GSAP).
  - Frameless, transparent, `always_on_top`, click-through aware, threejs.
- **Classification Engine**:
  - Preprocessing: Jina Reader API (`r.jina.ai`) for URLs + Metadata Parser for Videos.
  - Decision Core: TypeSafe `Jev` Model SDK (or HTTP Client) for sub-100ms multi-choice mapping.

## 3. Directory Structure Standards
```text
aether-hud/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── window/           # Transparent window styling & hotkey handlers
│   │   ├── storage/          # SQLite schema, migrations & CRUD
│   │   ├── parser/           # Jina Reader, Bilibili/YouTube metadata scrapers
│   │   ├── classifier/       # Jev client & choice resolution
│   │   ├── ingestion/        # Cron jobs (Feishu/Google Docs) & Axum Webhook
│   │   └── commands.rs       # Tauri IPC commands to frontend
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                      # Frontend UI
│   ├── components/
│   │   ├── SaoMenu/          # Radial / cascade navigation
│   │   ├── TagFilter/        # L3 Pill multi-select bar
│   │   ├── CardStream/       # Result cards list
│   │   └── QuickInput/       # Command palette style collector
│   ├── stores/
│   └── App.svelte
├── taxonomy.json             # Taxonomy configuration
└── PRD.md
```

## 4. Coding Conventions for Agents
1. **Window Transparency Rules (Tauri v2)**:
   - Always ensure `tauri.conf.json` sets `"transparent": true`, `"decorations": false`, `"shadow": false`.
   - On macOS, handle vibrancy if applicable; on Windows, enable transparent DWM composition.
2. **Fail-safe Classification Flow**:
   - Step 1: Input enters Inbox table with `status = "pending"`.
   - Step 2: Parser fetches content snapshot (timeout <= 4s).
   - Step 3: Run Jev classifier (`l1`, `l2`, `l3_tags`). If Jev is unreachable, fallback to default `(l1: "00_未分类", l2: "待人工归档")`.
   - Step 4: Update database row and notify frontend via Tauri Event `inbox-updated`.
3. **Database Performance**:
   - Use SQLite with WAL mode enabled (`PRAGMA journal_mode=WAL;`).
   - All queries filtering by `l1`, `l2` and `tags` must be fast; use indexed columns or JSON array indexing for `tags`.

## 5. Development Steps (Vibe Coding Guide)
- **Phase 1**: Initialize Tauri v2 minimal app, configure transparent frameless window + global shortcut (`Alt+Space` / `Option+Space`) toggle visibility.
- **Phase 2**: SQLite schema setup, read `taxonomy.json` into state.
- **Phase 3**: SAO HUD visual prototype (L1 cascade to L2, pill tags filter).
- **Phase 4**: Ingestion pipeline implementation (Axum local webhook server + Jina Reader integration + Jev classification pipeline).
- **Phase 5**: Cloud docs polling worker (Feishu Bitable token-based poller).

## Agent skills

### Issue tracker

Issues 跟踪在 GitHub Issues（kostya-sec/AetherHUD），使用 `gh` CLI 操作。See `docs/agents/issue-tracker.md`.

### Triage labels

使用五个默认标签：needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix。See `docs/agents/triage-labels.md`.

### Domain docs

Single-context：根目录 `GLOSSARY.md` + `docs/adr/`，按需懒创建。See `docs/agents/domain.md`.