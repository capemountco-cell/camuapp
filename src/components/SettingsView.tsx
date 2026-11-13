import { useCallback, useEffect, useState } from "react";

import { CHECK_INTERVALS } from "../lib/model";
import {
  backupSummary,
  deleteBackups,
  nativeAvailable,
  revealBackups,
  type BackupSummary,
  type SelfUpdate,
  type Settings,
} from "../lib/native";

function size(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return bytes > 0 ? "under 1 MB" : "0 MB";
}

function hourLabel(h: number): string {
  const d = new Date(2000, 0, 1, h);
  return d.toLocaleTimeString(undefined, { hour: "numeric" });
}

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const KEEP_DAYS = [7, 30, 90, 0];
const MAX_GB = [5, 10, 25, 0];

interface Props {
  settings: Settings;
  version: string | null;
  selfUpdate: SelfUpdate | null;
  checkingSelf: boolean;
  onChange: (next: Settings) => void;
  onCheckSelf: () => void;
  onInstallSelf: () => void;
  onLink: (url: string) => void;
}

function Toggle({ on, disabled, onChange, label }: { on: boolean; disabled?: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      className={`switch${on ? " on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!on)}
    >
      <span className="knob" />
    </button>
  );
}

function SettingRow({ title, detail, children }: { title: string; detail: string; children: React.ReactNode }) {
  return (
    <div className="setting">
      <div className="setting-text">
        <span className="setting-title">{title}</span>
        <span className="setting-detail">{detail}</span>
      </div>
      {children}
    </div>
  );
}

function Check({ checked, onChange, title, detail }: { checked: boolean; onChange: (v: boolean) => void; title: string; detail: string }) {
  return (
    <label className="setting sub">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="setting-text">
        <span className="setting-title">{title}</span>
        <span className="setting-detail">{detail}</span>
      </span>
    </label>
  );
}

export function SettingsView({ settings, version, selfUpdate, checkingSelf, onChange, onCheckSelf, onInstallSelf, onLink }: Props) {
  const set = (patch: Partial<Settings>) => onChange({ ...settings, ...patch });
  const [backups, setBackups] = useState<BackupSummary | null>(null);
  const [confirmAll, setConfirmAll] = useState(false);
  const loadBackups = useCallback(() => {
    if (nativeAvailable()) backupSummary().then(setBackups, () => {});
  }, []);
  useEffect(loadBackups, [loadBackups]);
  const remove = (scope: "deleted" | "all") => {
    setConfirmAll(false);
    deleteBackups(scope).then(loadBackups, loadBackups);
  };
  const auto = settings.autoUpdate && settings.checkEveryHours > 0;

  return (
    <div className="settings">
      <section className="settings-group">
        <h2>Checking</h2>
        <SettingRow title="Check automatically" detail="Camu checks in the background, even with the window closed.">
          <select
            className="select"
            value={settings.checkEveryHours}
            onChange={(e) => set({ checkEveryHours: Number(e.target.value) })}
            aria-label="Check automatically"
          >
            {CHECK_INTERVALS.map((i) => (
              <option key={i.hours} value={i.hours}>
                {i.label}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow title="Notify me" detail="Once per new version. Ignored apps stay quiet.">
          <Toggle label="Notify me" on={settings.notify} onChange={(notify) => set({ notify })} />
        </SettingRow>
        <SettingRow
          title="Install updates automatically"
          detail="After a scheduled check, install what Camu can install itself, with a backup first. App Store updates and apps that need their own updater are never automatic."
        >
          <Toggle
            label="Install updates automatically"
            on={settings.autoUpdate}
            disabled={settings.checkEveryHours === 0}
            onChange={(autoUpdate) => set({ autoUpdate })}
          />
        </SettingRow>
        {auto && (
          <div className="setting-subs">
            <Check
              checked={settings.autoSkipOpen}
              onChange={(autoSkipOpen) => set({ autoSkipOpen })}
              title="Skip apps that are open"
              detail="They update the next time you quit them, or with Update All."
            />
            <Check
              checked={settings.autoNoPassword}
              onChange={(autoNoPassword) => set({ autoNoPassword })}
              title="Never ask for my password in the background"
              detail="Updates that need an administrator password wait for you."
            />
            <Check
              checked={settings.askMajor}
              onChange={(askMajor) => set({ askMajor })}
              title="Ask first for major versions"
              detail="A jump like 5.x → 6.0 can need a new licence. These wait in Needs you, also for Update All."
            />
            <div className="setting sub">
              <input
                type="checkbox"
                checked={settings.quietHours !== null}
                onChange={(e) => set({ quietHours: e.target.checked ? { from: 22, to: 7 } : null })}
                aria-label="Only install during set hours"
              />
              <span className="setting-text">
                <span className="setting-title">Only install between</span>
                <span className="setting-detail">Automatic installs wait for these hours.</span>
              </span>
              {settings.quietHours && (
                <span className="hours">
                  <select
                    className="select"
                    aria-label="From"
                    value={settings.quietHours.from}
                    onChange={(e) => set({ quietHours: { ...settings.quietHours!, from: Number(e.target.value) } })}
                  >
                    {HOURS.map((h) => (
                      <option key={h} value={h}>
                        {hourLabel(h)}
                      </option>
                    ))}
                  </select>
                  <span className="faint">and</span>
                  <select
                    className="select"
                    aria-label="To"
                    value={settings.quietHours.to}
                    onChange={(e) => set({ quietHours: { ...settings.quietHours!, to: Number(e.target.value) } })}
                  >
                    {HOURS.map((h) => (
                      <option key={h} value={h}>
                        {hourLabel(h)}
                      </option>
                    ))}
                  </select>
                </span>
              )}
            </div>
          </div>
        )}
      </section>

      <section className="settings-group">
        <h2>Backups</h2>
        <SettingRow
          title={backups ? `${size(backups.totalBytes)} in ${backups.count} backup${backups.count === 1 ? "" : "s"}` : "Backups"}
          detail="Camu backs up each app before replacing it, so you can roll back."
        >
          <button type="button" className="btn" onClick={() => void revealBackups()}>
            Show in Finder
          </button>
        </SettingRow>
        <SettingRow title="Keep backups for" detail="Older backups are removed after each check.">
          <select
            className="select"
            aria-label="Keep backups for"
            value={settings.backupDays}
            onChange={(e) => set({ backupDays: Number(e.target.value) })}
          >
            {KEEP_DAYS.map((d) => (
              <option key={d} value={d}>
                {d === 0 ? "Forever" : `${d} days`}
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow title="Never use more than" detail="The oldest backups go first.">
          <select
            className="select"
            aria-label="Never use more than"
            value={settings.backupMaxGb}
            onChange={(e) => set({ backupMaxGb: Number(e.target.value) })}
          >
            {MAX_GB.map((g) => (
              <option key={g} value={g}>
                {g === 0 ? "No limit" : `${g} GB`}
              </option>
            ))}
          </select>
        </SettingRow>
        {backups && backups.count > 0 && (
          <SettingRow
            title="Remove backups"
            detail={
              backups.deletedAppsBytes > 0
                ? `${size(backups.deletedAppsBytes)} belongs to apps you've deleted.`
                : "Rolling back won't be possible for removed backups."
            }
          >
            <span className="hours">
              {backups.deletedAppsBytes > 0 && (
                <button type="button" className="btn" onClick={() => remove("deleted")}>
                  Of deleted apps
                </button>
              )}
              {confirmAll ? (
                <button type="button" className="btn danger" onClick={() => remove("all")}>
                  Delete all {backups.count}?
                </button>
              ) : (
                <button type="button" className="btn" onClick={() => setConfirmAll(true)}>
                  Delete all…
                </button>
              )}
            </span>
          </SettingRow>
        )}
      </section>

      <section className="settings-group">
        <h2>Where Camu lives</h2>
        <SettingRow title="Menu bar" detail="Shows the number of updates. Closing the window keeps Camu running there.">
          <Toggle label="Menu bar" on={settings.menuBar} onChange={(menuBar) => set({ menuBar })} />
        </SettingRow>
        <SettingRow title="Dock" detail="Hide the Dock icon to run from the menu bar only.">
          <Toggle
            label="Dock"
            on={settings.showInDock}
            disabled={!settings.menuBar}
            onChange={(showInDock) => set({ showInDock })}
          />
        </SettingRow>
      </section>

      <section className="settings-group">
        <h2>About</h2>
        <SettingRow
          title={`Camu ${version ?? ""}`.trim()}
          detail={
            selfUpdate
              ? `Camu ${selfUpdate.version} is available.${selfUpdate.canInstall ? "" : " Download it from camu.cc."}`
              : "No accounts, nothing that identifies you. Camu talks to the update sources it checks, and to camu.cc once to count the install."
          }
        >
          {selfUpdate ? (
            selfUpdate.canInstall ? (
              <button type="button" className="btn go" onClick={onInstallSelf}>
                Install and relaunch
              </button>
            ) : (
              <button
                type="button"
                className="btn"
                onClick={() => selfUpdate.pageUrl && onLink(selfUpdate.pageUrl)}
                disabled={!selfUpdate.pageUrl}
              >
                Download {selfUpdate.version}
              </button>
            )
          ) : (
            <button type="button" className="btn" onClick={onCheckSelf} disabled={checkingSelf}>
              {checkingSelf ? "Checking…" : "Check for Camu updates"}
            </button>
          )}
        </SettingRow>
        {(() => {
          const skipped = Object.keys(settings.skipped).length;
          const snoozed = Object.keys(settings.snoozed).length;
          const pinned = Object.keys(settings.pinned).length;
          const any = settings.ignored.length + settings.beta.length + skipped + snoozed + pinned > 0;
          return any ? (
            <SettingRow
              title="Per-app choices"
              detail={[
                settings.ignored.length && `${settings.ignored.length} ignored`,
                settings.beta.length && `${settings.beta.length} on beta channels`,
                skipped && `${skipped} skipped`,
                snoozed && `${snoozed} reminding later`,
                pinned && `${pinned} staying on a version`,
              ]
                .filter(Boolean)
                .join(", ")}
            >
              <button
                type="button"
                className="btn"
                onClick={() => set({ ignored: [], beta: [], skipped: {}, snoozed: {}, pinned: {} })}
              >
                Reset
              </button>
            </SettingRow>
          ) : null;
        })()}
      </section>
    </div>
  );
}
