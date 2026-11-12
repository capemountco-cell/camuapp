import { installLabel, isBusy, otherReason, phaseLabel, sourceLabel, type Row } from "../lib/model";
import type { Backup, Phase } from "../lib/native";
import { AppIcon } from "./AppIcon";
import { Close, External, EyeOff, Folder, Undo } from "./Icons";
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
}

/** The side panel: properties first, release notes below, like an issue view. */
export function DetailPanel(props: Props) {
  const { row, phase, backup, ignored, beta } = props;
  const { app, report } = row;
  const best = report?.best;
  const hasUpdate = report?.status === "updateAvailable" && phase?.phase !== "done";
  const busy = isBusy(phase);

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
        {hasUpdate && (
          <button type="button" className="btn go wide" onClick={props.onUpdate} disabled={busy}>
            {busy ? phaseLabel(phase!) : `Update to ${best?.version}`}
            {!busy && <Kbd keys={["U"]} />}
          </button>
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
        </section>
      </div>
    </aside>
  );
}
