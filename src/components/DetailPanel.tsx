import { useState } from "react";

import { holdLabel, installLabel, isBusy, noticeLabel, otherReason, phaseLabel, sourceLabel, type Row } from "../lib/model";
import type { Backup, Choice, Phase } from "../lib/native";
import { AppIcon } from "./AppIcon";
import { Close, External, EyeOff, Flag, Folder, More, Undo } from "./Icons";
import { Kbd } from "./Kbd";

interface Props {
  row: Row;
  phase: Phase | undefined;
  backup: Backup | undefined;
  ignored: boolean;
  beta: boolean;
  onClose: () => void;
  onUpdate: () => void;
  onIgnore: () => void;
  onBeta: () => void;
  onReveal: () => void;
  onLink: (url: string) => void;
  onRollBack: () => void;
  /** Skip this version, remind later, stay on this major, or clear. */
  onChoice: (choice: Choice) => void;
  /** Send camu.cc a report of what Camu saw, for a wrong version. */
  onReport: () => void;
  /** Show a file in Finder: another copy of this app. */
  onRevealPath: (path: string) => void;
}

/** The side panel: properties first, release notes below, like an issue view. */
export function DetailPanel(props: Props) {
  const { row, phase, backup, ignored, beta } = props;
  const { app, report } = row;
  const best = report?.best;
  const hasUpdate = report?.status === "updateAvailable" && phase?.phase !== "done" && !report.hold;
  const busy = isBusy(phase);
  const [menu, setMenu] = useState(false);
  const installedMajor = app.version.match(/\d+/)?.[0];
  const choose = (c: Choice) => {
    setMenu(false);
    props.onChoice(c);
  };

  const status = phase
    ? phaseLabel(phase)
    : report?.status === "updateAvailable"
      ? "Update available"
      : report?.status === "upToDate"
        ? "Up to date"
        : otherReason(row);

  return (
    <aside className="panel" aria-label={`${app.name} details`}>
      <header className="panel-head">
        <AppIcon app={app} />
        <h2>{app.name}</h2>
        <button type="button" className="icon-btn" onClick={props.onClose} title="Close (Esc)">
          <Close />
        </button>
      </header>

      <div className="panel-body">
        {report?.hold && (
          <div className="held">
            <span>{holdLabel(report.hold)}</span>
            <button type="button" className="btn" onClick={() => choose("clear")}>
              Show this update again
            </button>
          </div>
        )}
        {hasUpdate && (
          <div className="update-row">
            <button
              type="button"
              className="btn go wide"
              onClick={props.onUpdate}
              disabled={busy || report?.notice?.kind === "needsMacOs"}
            >
              {busy
                ? phaseLabel(phase!)
                : report?.install.kind === "vendor"
                  ? `Open ${report.install.name}`
                  : `Update to ${best?.version}`}
              {!busy && report?.install.kind !== "vendor" && <Kbd keys={["U"]} />}
            </button>
            <div className="menu-anchor">
              <button
                type="button"
                className="btn icon-only"
                aria-label="More update options"
                aria-expanded={menu}
                onClick={() => setMenu((m) => !m)}
              >
                <More />
              </button>
              {menu && (
                <div className="popmenu" role="menu" aria-label="Update options">
                  <button type="button" role="menuitem" onClick={() => choose("skip")}>
                    Skip version {best?.version}
                  </button>
                  <button type="button" role="menuitem" onClick={() => choose("snooze")}>
                    Remind me in 30 days
                  </button>
                  {installedMajor && report?.major && (
                    <button type="button" role="menuitem" onClick={() => choose("pin")}>
                      Stay on version {installedMajor}.x
                    </button>
                  )}
                  <hr />
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setMenu(false);
                      props.onIgnore();
                    }}
                  >
                    Never update {app.name}
                  </button>
                </div>
              )}
            </div>
          </div>
        )}
        {report?.notice && report.notice.kind !== "setapp" && (
          <p className="notice">{noticeLabel(report.notice)}</p>
        )}
        {(row.app.otherCopies?.length ?? 0) > 0 && (
          <p className="notice">
            {row.app.otherCopies!.length === 1 ? "Another copy" : `${row.app.otherCopies!.length} other copies`} of{" "}
            {row.app.name} {row.app.otherCopies!.length === 1 ? "is" : "are"} installed. Camu shows and updates the one
            at {row.app.path}.{" "}
            {row.app.otherCopies!.map((p) => (
              <button key={p} type="button" className="link-button" onClick={() => props.onRevealPath(p)}>
                Show {p} in Finder
              </button>
            ))}
          </p>
        )}
        {report?.major && hasUpdate && (
          <p className="notice warn">A new major version. It may need a new licence or change how the app works.</p>
        )}

        <dl className="props">
          <dt>Status</dt>
          <dd className={phase?.phase === "failed" ? "danger" : undefined}>{status}</dd>
          <dt>Installed</dt>
          <dd className="mono">
            {app.version}
            {app.build && app.build !== app.version && <span className="faint"> ({app.build})</span>}
          </dd>
          {best && (
            <>
              <dt>Latest</dt>
              <dd className={`mono${best.isUpdate ? " accent" : ""}`}>{best.version}</dd>
              <dt>Source</dt>
              <dd>
                <span className={`src src-${best.source}`}>{sourceLabel[best.source]}</span>
              </dd>
            </>
          )}
          {report && hasUpdate && (
            <>
              <dt>Installs via</dt>
              <dd>{installLabel(report.install)}</dd>
            </>
          )}
          <dt>Bundle ID</dt>
          <dd className="mono faint selectable">{app.bundleId}</dd>
        </dl>

        {report && report.findings.length + report.errors.length > 1 && (
          <section className="section">
            <h3>Sources</h3>
            <ul className="sources">
              {report.findings.map((f) => (
                <li key={f.source}>
                  <span className={`src src-${f.source}`}>{sourceLabel[f.source]}</span>
                  <span className={`mono${f.isUpdate ? " accent" : ""}`}>{f.version}</span>
                </li>
              ))}
              {report.errors.map((e) => (
                <li key={`e-${e.source}`} title={e.message}>
                  <span>{sourceLabel[e.source]}</span>
                  <span className="faint">no answer</span>
                </li>
              ))}
            </ul>
          </section>
        )}

        <section className="section">
          <h3>Release notes</h3>
          {best?.releaseNotes ? (
            <pre className="notes">{best.releaseNotes}</pre>
          ) : (
            <p className="faint">None published.</p>
          )}
          {best?.releaseNotesUrl && (
            <button type="button" className="link" onClick={() => props.onLink(best.releaseNotesUrl!)}>
              <External /> Read online
            </button>
          )}
        </section>

        <section className="section actions">
          <button type="button" className="menu-item" onClick={props.onReveal}>
            <Folder /> Show in Finder
          </button>
          {best?.pageUrl && (
            <button type="button" className="menu-item" onClick={() => props.onLink(best.pageUrl!)}>
              <External /> {best.source === "appStore" ? "Open in App Store" : "Open website"}
            </button>
          )}
          {backup && (
            <button type="button" className="menu-item" onClick={props.onRollBack} disabled={busy}>
              <Undo /> Roll back to {backup.version}
            </button>
          )}
          <button type="button" className="menu-item" onClick={props.onIgnore}>
            <EyeOff /> {ignored ? "Stop ignoring" : "Ignore updates"}
            <Kbd keys={["I"]} />
          </button>
          {app.sparkleFeed && (
            <label className="menu-item">
              <input type="checkbox" checked={beta} onChange={props.onBeta} /> Include beta versions
            </label>
          )}
          {report && (report.findings.length > 0 || report.errors.length > 0) && (
            <button type="button" className="menu-item" onClick={props.onReport}>
              <Flag /> Version looks wrong? Report it
            </button>
          )}
        </section>
      </div>
    </aside>
  );
}
