import { actionLabel, otherReason, sourceLabel, type Row } from "../lib/model";
import type { Report } from "../lib/native";
import { AppIcon } from "./AppIcon";

interface Props {
  row: Row;
  open: boolean;
  ignored: boolean;
  beta: boolean;
  onToggleOpen: () => void;
  onAction: (report: Report) => void;
  onIgnore: () => void;
  onBeta: () => void;
  onReveal: () => void;
  onLink: (url: string) => void;
}

export function AppRow({ row, open, ignored, beta, onToggleOpen, onAction, onIgnore, onBeta, onReveal, onLink }: Props) {
  const { app, report } = row;
  const best = report?.best;
  const hasUpdate = report?.status === "updateAvailable";
  const action = hasUpdate && report ? actionLabel(report.install) : null;

  return (
    <li className={`row${open ? " open" : ""}`}>
      <div
        className="row-main"
        role="button"
        tabIndex={0}
        aria-expanded={open}
        onClick={onToggleOpen}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggleOpen();
          }
        }}
      >
        <AppIcon app={app} />
        <div className="row-name">
          <span className="name">{app.name}</span>
          {best && <span className="source">{sourceLabel[best.source]}</span>}
        </div>
        <div className="row-version">
          {hasUpdate && best ? (
            <>
              <span className="from">{app.version}</span>
              <span className="arrow" aria-label="to">
                →
              </span>
              <span className="to">{best.version}</span>
            </>
          ) : report?.status === "upToDate" || report?.status === undefined ? (
            <span className="from">{app.version}</span>
          ) : (
            <span className="reason">{otherReason(row)}</span>
          )}
        </div>
        <div className="row-action">
          {action && report && (
            <button
              type="button"
              className="pill"
              onClick={(e) => {
                e.stopPropagation();
                onAction(report);
              }}
            >
              {action}
            </button>
          )}
          {!report && <span className="spinner" aria-label="Checking" />}
        </div>
      </div>

      {open && (
        <div className="details">
          {best?.releaseNotes ? (
            <pre className="notes">{best.releaseNotes}</pre>
          ) : (
            <p className="muted small">{hasUpdate ? "No release notes published." : " "}</p>
          )}
          <dl className="facts">
            <div>
              <dt>Installed</dt>
              <dd>
                {app.version}
                {app.build && app.build !== app.version ? ` (${app.build})` : ""}
              </dd>
            </div>
            {report?.findings.map((f) => (
              <div key={f.source}>
                <dt>{sourceLabel[f.source]}</dt>
                <dd className={f.isUpdate ? "accent" : undefined}>{f.version}</dd>
              </div>
            ))}
            {report?.errors.map((e) => (
              <div key={`err-${e.source}`}>
                <dt>{sourceLabel[e.source]}</dt>
                <dd className="muted" title={e.message}>
                  no answer
                </dd>
              </div>
            ))}
          </dl>
          <div className="detail-actions">
            {best?.releaseNotesUrl && (
              <button type="button" className="link" onClick={() => onLink(best.releaseNotesUrl!)}>
                Release notes ↗
              </button>
            )}
            {best?.pageUrl && best.pageUrl !== best.releaseNotesUrl && (
              <button type="button" className="link" onClick={() => onLink(best.pageUrl!)}>
                {best.source === "appStore" ? "App Store ↗" : "Website ↗"}
              </button>
            )}
            <button type="button" className="link" onClick={onReveal}>
              Show in Finder
            </button>
            <span className="spacer" />
            {app.sparkleFeed && (
              <label className="check">
                <input type="checkbox" checked={beta} onChange={onBeta} /> Betas
              </label>
            )}
            <button type="button" className="link" onClick={onIgnore}>
              {ignored ? "Stop ignoring" : "Ignore"}
            </button>
          </div>
        </div>
      )}
    </li>
  );
}
