# AGENTS.md — Development Blueprint for AI Coding Agents

> 本文件已按 [PRD v2](PRD.md) 与 [AetherHUD v1 实现规格](https://github.com/kostya-sec/AetherHUD/issues/24) 对齐（2026-10-21 收口）。施工前必读：`PRD.md`、`GLOSSARY.md`、本文件、`docs/spec/` 中与任务相关的详规。

## 1. Project Context & Vision
- **Project Name**: `AetherHUD` (SAO-Style Resource Nexus).
- **Core Goal**: A ultra-lightweight, cross-platform (Windows & macOS) desktop floating HUD application built with **Rust + Tauri v2**.
- **UX Paradigm**: SAO Utils-inspired radial/cascading floating interface.
  - Hidden by default. Summoned via Global Hotkey. Dismissed (收起) on `ESC` or blur; never quits from window close.
  - Multi-source ingestion (Feishu Bitable polling, Chatbot Webhooks, HUD Quick Input).
  - 2+1 Tier Taxonomy: L1/L2 rigid hierarchy + L3 dynamic clickable Tag Pills.
  - Fully keyboard reachable (five focus zones + search bubble); Chinese IME safe.

## 2. Tech Stack & Architecture
- **Framework**: Tauri v2 (`core` + `plugins`: global-shortcut, autostart, single-instance, tray-icon; macOS NSPanel via tauri-nspanel; vibrancy via window-vibrancy).
- **Core Backend**: Rust (latest), Cargo workspace.
  - `crates/aether-core`: the single domain seam — storage/migrations, dedupe & canonicalization, fetch chain, ingestion (local/webhook/Feishu), pipeline worker, taxonomy overlay, tag governance, corrections, settings & Keychain abstraction, update-check state. Adapters (clock/HTTP/Keychain/FS) injected for tests.
  - `crates/aether-classifier`: the only classification path — effective-taxonomy injection, V-C4 prompt, output validation + one repair retry, OpenAI-compatible provider, retry semantics. Shared by worker and eval.
  - `crates/aether-eval`: offline evaluation CLI (metrics, reports, baselines, correction candidates, few-shot dedupe).
  - `src-tauri`: thin OS shell only — transparent window, show/dismiss state machine, tray, hotkeys, mouse gesture, single instance, autostart, DPI/multi-monitor, click-through polling, IPC commands wrapping `aether-core`.
  - Storage: `rusqlite` (bundled) with FTS5; `tokio`; `reqwest`; `axum` (localhost webhook); `keyring` (OS Keychain); `rusqlite_migration`.
- **Frontend / HUD Layer**: React 19 + TypeScript + Vite + Tailwind CSS 4 + motion + three.js + matter.js (the accepted #7 prototype stack; **supersedes the old Svelte/Vue blueprint**).
  - Framework-free HUD interaction model (S0–S3 view state, focus zones, keyboard routing, IME guard, filtering) tested with Vitest; React components render only.
  - Frameless, transparent, `always_on_top`, click-through aware.
- **Classification Engine**: OpenAI-compatible Chat Completions, default DashScope `qwen-flash`; V-C4 prompt with effective taxonomy injection, field-by-field validation, 1 repair retry, abstain + classification suggestion. No Jev model SDK.
- **Fetching**: degradation chain — video metadata (Bilibili WBI / YouTube oEmbed) → direct `reqwest` + `dom_smoothie` (8s) → Jina Reader (20s, proxy-capable, optional key) → replaceable backend (interface only) → title-only fallback (`metadata_status=partial`).

## 3. Directory Structure Standards
```text
AetherHUD/
├── crates/
│   ├── aether-core/          # Domain seam: storage, ingestion, fetch, worker, taxonomy, settings
│   ├── aether-classifier/    # Shared classification path (prompt/validate/provider/retry/engine)
│   └── aether-eval/          # Offline eval CLI (read effective taxonomy, local eval sets)
├── src-tauri/                # Thin Tauri shell: window/tray/hotkey/gesture/DPI/IPC commands
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── window/           # Transparent window, show/dismiss state machine, NSPanel, DPI
│   │   ├── tray/             # Tray icon, menu, tooltip status
│   │   ├── hotkey/           # Global shortcut register/rebind/conflict fallback
│   │   ├── gesture/          # Windows SAO mouse gesture (opt-in, default off)
│   │   └── commands.rs       # Thin IPC wrappers over aether-core
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                      # Frontend (React 19)
│   ├── hud-model/            # Framework-free state machine, keyboard routing, selectors
│   ├── components/
│   │   ├── SaoRadialMenu/    # Pixel-ball radial menu + ball expand rows
│   │   ├── L2Panel/          # Cascading L2 panel
│   │   ├── TagFilterBar/     # L3 pill filter (AND/OR, visibility rules)
│   │   ├── CardStream/       # Result cards + view switcher
│   │   ├── QuickInput/       # Ingest modal
│   │   ├── DetailModal/      # Entry detail, corrections, suggestions
│   │   └── Settings/         # Ten-section settings layer
│   └── App.tsx
├── docs/
│   ├── spec/                 # Construction specs (SQLite, worker, keyboard, quality, taxonomy, keys, dist)
│   ├── adr/                  # ADR-0001..0003
│   └── eval/                 # Eval set templates & local reports (no real data in git)
├── taxonomy.json             # Read-only base taxonomy (current 1.1.0)
├── PRD.md                    # Product baseline v2
└── GLOSSARY.md               # Domain language (authoritative terms)
```

## 4. Coding Conventions for Agents
1. **Window Transparency Rules (Tauri v2)**:
   - `tauri.conf.json`: `"transparent": true`, `"decorations": false`, `"shadow": false`, `"noRedirectionBitmap": true` (Windows), `"macOSPrivateApi": true`.
   - Never use `hide()`/`show()` for HUD visibility: use off-screen docking + `set_position` (dismiss) / `show → set_position → set_focus` (summon). macOS uses NSPanel + `window_did_resign_key`; blur dismiss needs ~250ms grace. Click-through = whole-window `set_ignore_cursor_events` + cursor polling hitboxes. Convert physical↔logical with scaleFactor; clamp to one monitor.
2. **Fail-safe Ingestion & Classification Flow**:
   - Step 1: Input enters `entries` with `pipeline_status = pending` and a `pipeline_jobs` row, in one transaction; notify frontend via the `inbox-updated` event.
   - Step 2: Worker (single, serial) fetches with the degradation chain (direct 8s / Jina 20s), stores `body_md` ≤1500 chars + `excerpt` ≤500.
   - Step 3: `aether-classifier` runs V-C4 with the **effective taxonomy**; if unreachable/invalid after one repair retry, fall back to `(l1_inbox, l2_manual_review)` and apply the #11 retry/terminal rules.
   - Step 4: Persist classification, suggestion, tags and entry status in the same transaction as job deletion; frontend refreshes via events.
3. **Database Rules**:
   - SQLite WAL (`journal_mode=WAL`), `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`; migrations append-only via `rusqlite_migration`.
   - FTS5 **trigram**, excerpt-bounded; `<3` chars use `LIKE` fallback; user input is segmented/quoted and never interpolated raw into `MATCH`.
   - L3 tags are normalized entities (`tags`/`tag_aliases`/`entry_tags`), never JSON columns for filterable fields; visibility counts computed at query time.
   - Back up with `VACUUM INTO` before migrations (keep 5), together with the taxonomy overlay snapshot.
4. **Security & Privacy Rules**:
   - API keys/secrets only in OS Keychain (service `aetherhud`); DB/settings store only `keychain://aetherhud/<account>` refs. Never log keys, signatures, or raw bodies.
   - Webhook binds `127.0.0.1` only, random long path, raw-bytes-first signature verification per platform, idempotency by platform message id. Public ingress only via Cloudflare named tunnel.
   - No telemetry, no crash reporting, no analytics SDK; network only for fetch/classify/Feishu sync/update check/user-opened links.
5. **Domain Language**: use `GLOSSARY.md` terms (条目/收件箱/未分类/封存/停用/弃权/分类建议/采纳/升格/自建节点/覆盖层/生效分类树/纠偏/唤出/收起/退出). Avoid the listed synonyms.

## 5. Development Steps (Vibe Coding Guide)
- **Phase 1**: Scaffold Tauri v2 workspace shell, transparent frameless window, show/dismiss state machine, tray, single instance, hotkey (Windows Alt+\`, macOS Option+Space), DPI handling.
- **Phase 2**: `aether-core` storage: migration v1/v2, entries/tags/jobs/sync tables, FTS5 trigram + triggers, dedupe, backups; load effective taxonomy (base + overlay).
- **Phase 3**: HUD frontend extraction from the #7 prototype (React 19), framework-free `hud-model`, five-focus-zone keyboard model, S0–S3 views, card stream + 50/page pagination.
- **Phase 4**: Ingestion pipeline: local quick input, axum webhook (Telegram/Feishu/DingTalk), Feishu Bitable poller, fetch degradation chain, shared classifier worker, offline suspension/retry.
- **Phase 5**: Taxonomy overlay & tag governance UI, settings layer (ten sections), Keychain/proxy, corrections & suggestion adoption/promotion, update check, packaging (NSIS / universal dmg) and docs alignment.

## Agent skills

### Issue tracker

Issues 跟踪在 GitHub Issues（kostya-sec/AetherHUD），使用 `gh` CLI 操作。See `docs/agents/issue-tracker.md`.

### Triage labels

使用五个默认标签：needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix。See `docs/agents/triage-labels.md`.

### Domain docs

Single-context：根目录 `GLOSSARY.md` + `docs/adr/`，按需懒创建。See `docs/agents/domain.md`.
