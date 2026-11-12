import { CHECK_INTERVALS } from "../lib/model";
import type { SelfUpdate, Settings } from "../lib/native";

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

export function SettingsView({ settings, version, selfUpdate, checkingSelf, onChange, onCheckSelf, onInstallSelf, onLink }: Props) {
  const set = (patch: Partial<Settings>) => onChange({ ...settings, ...patch });

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
              ? `Camu ${selfUpdate.version} is available.${selfUpdate.canInstall ? "" : " This copy isn't signed with a Developer ID, so download it from the release page."}`
              : "No accounts, no analytics. Camu talks only to the update sources it checks."
          }
        >
          {selfUpdate ? (
            selfUpdate.canInstall ? (
              <button type="button" className="btn primary" onClick={onInstallSelf}>
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
        {(settings.ignored.length > 0 || settings.beta.length > 0) && (
          <SettingRow
            title="Per-app choices"
            detail={`${settings.ignored.length} ignored, ${settings.beta.length} on beta channels.`}
          >
            <button type="button" className="btn" onClick={() => set({ ignored: [], beta: [] })}>
              Reset
            </button>
          </SettingRow>
        )}
      </section>
    </div>
  );
}
