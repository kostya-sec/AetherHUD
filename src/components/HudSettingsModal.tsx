import { useEffect, useRef } from "react";
import {
  SETTINGS_SECTIONS,
  type CredentialKind,
  type SettingsSectionId,
  type SettingsState,
  type ValidationTarget,
} from "../hud-model";
import { HudCredentialsPane } from "./HudCredentialsPane";
import { modalSurfaceStyle } from "./hudSurface";

interface HudSettingsModalProps {
  settings: SettingsState;
  hudOpacity: number;
  onClose: () => void;
  onSelectSection: (section: SettingsSectionId) => void;
  /** 返回 true = 保存成功（调用方据此清空该行输入框）。 */
  onSaveCredential: (kind: CredentialKind, secret: string) => Promise<boolean>;
  onClearCredential: (kind: CredentialKind) => Promise<boolean>;
  onValidate: (target: ValidationTarget) => Promise<void>;
  onQuit: () => void;
}

/**
 * 设置弹层（#24 §8 / #29）：左侧十区导航 + 右侧内容。
 *
 * 本票只有「凭证」分区落地（见 HudCredentialsPane）；其余九区显示占位（分区职责与
 * 后续票在 constants 的 SETTINGS_SECTIONS）。底部提供退出入口（#24 §10：退出只从
 * 托盘菜单与设置底部）。
 *
 * 交互约定：Esc 关闭并回到 HUD 原处（全局键位分层在 hud-model）；弹层不因点击
 * 背景关闭，避免误丢正在输入的密钥。弹层透明度 = 全局透明度 + 3%。
 */
export function HudSettingsModal({
  settings,
  hudOpacity,
  onClose,
  onSelectSection,
  onSaveCredential,
  onClearCredential,
  onValidate,
  onQuit,
}: HudSettingsModalProps) {
  const dialogRef = useRef<HTMLElement>(null);
  const sectionMeta =
    SETTINGS_SECTIONS.find((section) => section.id === settings.section) ??
    SETTINGS_SECTIONS[0];

  // 打开即把焦点放进弹层（窗口本身已有焦点）；Tab 顺序从标题行 × 开始。
  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  return (
    <div
      data-hud-hitbox
      data-hud-settings-modal
      className="fixed inset-0 z-40 flex items-center justify-center bg-stone-950/60 p-4"
    >
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="设置"
        tabIndex={-1}
        data-hud-hitbox
        className="pixel-box flex h-full w-full max-w-4xl flex-col rounded-xl border border-stone-700/90 p-4 outline-none"
        style={modalSurfaceStyle(hudOpacity)}
      >
        <header className="mb-3 flex items-center justify-between gap-3 border-b border-stone-800/80 pb-2">
          <div className="flex items-baseline gap-3">
            <h2 className="font-mono text-xs uppercase tracking-wider text-amber-300/90">
              设置
            </h2>
            <span className="font-mono text-[10px] text-stone-500">
              十区导航 · 本票落地「凭证」
            </span>
          </div>
          <button
            type="button"
            data-hud-hitbox
            onClick={onClose}
            title="关闭设置（Esc）"
            aria-label="关闭设置"
            className="pixel-box-sm hud-focus-ring flex h-5 w-5 items-center justify-center rounded border border-stone-700 text-stone-400 transition-colors hover:border-amber-400 hover:text-amber-300"
          >
            <span aria-hidden="true" className="text-xs leading-none">
              ×
            </span>
          </button>
        </header>

        <div className="flex min-h-0 flex-1 gap-4">
          <nav aria-label="设置分区" className="w-40 shrink-0 border-r border-stone-800/80 pr-2">
            <ul className="flex flex-col gap-1">
              {SETTINGS_SECTIONS.map((section) => {
                const active = section.id === settings.section;
                return (
                  <li key={section.id}>
                    <button
                      type="button"
                      data-hud-hitbox
                      data-settings-section={section.id}
                      aria-current={active ? "page" : undefined}
                      onClick={() => onSelectSection(section.id)}
                      className={`flex w-full items-center justify-between gap-1 rounded border px-2.5 py-1.5 text-left font-mono text-[11px] transition-colors ${
                        active
                          ? "border-amber-500/70 bg-amber-500/15 text-amber-200"
                          : "border-transparent text-stone-300 hover:border-stone-700 hover:text-white"
                      }`}
                    >
                      <span>{section.label}</span>
                      {!section.implemented && (
                        <span className="text-[9px] text-stone-600">占位</span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          </nav>

          <div className="min-h-0 flex-1 overflow-y-auto pr-1" data-settings-pane={sectionMeta.id}>
            <div className="mb-3">
              <h3 className="font-mono text-sm text-stone-100">{sectionMeta.label}</h3>
              <p className="mt-0.5 font-mono text-[10px] text-stone-500">
                {sectionMeta.summary}
              </p>
            </div>

            {sectionMeta.id === "credentials" ? (
              <HudCredentialsPane
                credentials={settings.credentials}
                onSaveCredential={onSaveCredential}
                onClearCredential={onClearCredential}
                onValidate={onValidate}
              />
            ) : (
              <div
                data-settings-placeholder={sectionMeta.id}
                className="flex h-56 flex-col items-center justify-center gap-2 rounded border border-dashed border-stone-700/70 text-center"
              >
                <p className="font-mono text-xs text-stone-400">
                  「{sectionMeta.label}」分区占位
                </p>
                <p className="max-w-md font-mono text-[10px] leading-relaxed text-stone-500">
                  该分区在后续票接入；本票只实现「凭证」区与弹层骨架。
                </p>
              </div>
            )}
          </div>
        </div>

        <footer className="mt-3 flex items-center justify-between gap-3 border-t border-stone-800/80 pt-2">
          <span className="font-mono text-[10px] text-stone-500">
            密钥只存系统钥匙串（service aetherhud） · 数据库与日志只留 keychain:// 引用
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              data-hud-hitbox
              data-settings-quit
              onClick={onQuit}
              title="退出 AetherHUD（立即结束进程，不等待收尾）"
              className="pixel-box-sm hud-focus-ring rounded border border-stone-700 px-2.5 py-1 font-mono text-[10px] text-stone-400 transition-colors hover:border-rose-500/70 hover:text-rose-200"
            >
              退出 AetherHUD
            </button>
            <span className="font-mono text-[10px] text-stone-500">Esc 关闭</span>
          </div>
        </footer>
      </section>
    </div>
  );
}
