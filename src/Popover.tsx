import { useCallback, useEffect, useState } from "react";

import { AppIcon } from "./components/AppIcon";
import { CheckCircle, Gear, Refresh, Wordmark } from "./components/Icons";
import { isBusy, needsYou, phaseLabel } from "./lib/model";
import {
  checkUpdates,
  events,
  getSettings,
  onReport,
  onUpdate,
  showMain,
  snapshot,
  updateApps,
  type Phase,
  type Report,
  type Snapshot,
} from "./lib/native";

function ago(secs: number | null): string {
  if (!secs) return "Not checked yet";
  const minutes = Math.round((Date.now() / 1000 - secs) / 60);
  if (minutes < 1) return "Checked just now";
  if (minutes < 60) return `Checked ${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `Checked ${hours}h ago` : `Checked ${new Date(secs * 1000).toLocaleDateString()}`;
}

/**
 * The panel under the menu bar icon: what needs updating, an Update button
 * per app, Update All, and a way into the main window. Same black, quiet
 * style as the window.
 */
export function Popover() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [ignored, setIgnored] = useState<string[]>([]);
  const [askMajor, setAskMajor] = useState(true);
  const [phases, setPhases] = useState<Map<string, Phase>>(new Map());

  const refresh = useCallback(() => {
    snapshot().then(setSnap, () => {});
    getSettings().then(
      (s) => {
        setIgnored(s.ignored);
        setAskMajor(s.askMajor);
      },
      () => {},
    );
  }, []);

  useEffect(() => {
    document.documentElement.classList.add("is-popover");
    refresh();
    const subs = [
      events.popoverShown(refresh),
      events.checkStarted(() => {
        setPhases(new Map());
        refresh();
      }),
      onReport(() => refresh()),
      events.checkFinished(refresh),
      events.updateStarted(refresh),
      onUpdate(({ id, ...phase }) => setPhases((m) => new Map(m).set(id, phase as Phase))),
      events.updateFinished(refresh),
    ];
    return () => subs.forEach((p) => void p.then((stop) => stop()));
  }, [refresh]);

  const pending: Report[] = (snap?.reports ?? []).filter(
    (r) => r.status === "updateAvailable" && !r.hold && !ignored.includes(r.app.bundleId),
  );
  // Update All leaves what needs the user (major versions, newer macOS,
  // the developer's own updater) for the main window.
  const waiting = pending.filter((r) => {
    const p = phases.get(r.id);
    return p?.phase !== "done" && !isBusy(p) && !needsYou(r, askMajor);
  });
  const reports = snap?.reports ?? [];
  const failed = reports.filter((r) => r.status === "failed").length;
  const offline = !snap?.checking && reports.length > 0 && failed / reports.length >= 0.5;
  const busy = snap?.updating || [...phases.values()].some(isBusy);
  const update = (ids: string[]) => void updateApps(ids).catch(() => {});

  return (
    <div className="pop">
      <header className="pop-head">
        <Wordmark height={12} className="wordmark pop-title" />
        <span className="spacer" />
        <button
          type="button"
          className="icon-btn"
          title="Check for updates"
          aria-label="Check for updates"
          disabled={snap?.checking || busy}
          onClick={() => void checkUpdates().catch(() => {})}
        >
          <Refresh className={snap?.checking ? "i spin" : "i"} />
        </button>
        <button type="button" className="icon-btn" title="Settings" aria-label="Settings" onClick={() => void showMain("settings")}>
          <Gear />
        </button>
      </header>

      <div className="pop-body">
        {snap === null ? null : pending.length === 0 ? (
          <div className="pop-empty">
            <CheckCircle className="i pop-empty-icon" />
            <p>
              {snap.checking
                ? "Checking for updates…"
                : offline
                  ? `Couldn't check ${failed} of ${reports.length} apps. You may be offline.`
                  : "Every app is up to date"}
            </p>
          </div>
        ) : (
          <ul className="pop-list">
            {pending.map((r) => {
              const phase = phases.get(r.id);
              return (
                <li key={r.id} className="pop-row">
                  <AppIcon app={r.app} />
                  <div className="pop-name">
                    <span className="name">{r.app.name}</span>
                    {phase ? (
                      <span className={`pop-sub p-${phase.phase}`}>{phaseLabel(phase)}</span>
                    ) : (
                      <span className="pop-sub">
                        {r.app.version} → <span className="to">{r.best?.version}</span>
                      </span>
                    )}
                  </div>
                  {needsYou(r, askMajor) ? (
                    <button type="button" className="btn small" onClick={() => void showMain("updates")}>
                      Review
                    </button>
                  ) : !phase || phase.phase === "failed" ? (
                    <button type="button" className="btn small" disabled={busy} onClick={() => update([r.id])}>
                      {r.install.kind === "appStore" ? "App Store" : "Update"}
                    </button>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <footer className="pop-foot">
        <div className="pop-status">
          <span>{snap?.checking ? "Checking…" : ago(snap?.lastCheck ?? null)}</span>
          <span className="faint">
            {snap?.reports.length ?? 0} apps · {pending.length} with updates
          </span>
        </div>
        <button type="button" className="btn" onClick={() => void showMain()}>
          Open Camu
        </button>
        {waiting.length > 0 && (
          <button type="button" className="btn go" disabled={busy} onClick={() => update(waiting.map((r) => r.id))}>
            Update all {waiting.length}
          </button>
        )}
      </footer>
    </div>
  );
}
