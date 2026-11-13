import type { Row } from "../lib/model";
import type { Backup, HistoryEntry } from "../lib/native";
import { AppIcon } from "./AppIcon";
import { CheckCircle, External, Undo, Warning } from "./Icons";

interface Props {
  entries: HistoryEntry[];
  /** Current rows, to find the app and whether it can be retried. */
  rows: Row[];
  backups: Backup[];
  onRetry: (row: Row) => void;
  onRollBack: (row: Row) => void;
}

function dayLabel(secs: number, now = new Date()): string {
  const d = new Date(secs * 1000);
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((start(now) - start(d)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric" });
}

function timeLabel(secs: number): string {
  return new Date(secs * 1000).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

const SOURCE: Record<string, string> = {
  sparkle: "Sparkle",
  electron: "Electron",
  appStore: "App Store",
  github: "GitHub",
  homebrew: "Homebrew",
};

function describe(e: HistoryEntry): string {
  const how = e.how === "automatic" ? "automatic" : "by you";
  const source = e.source ? ` · ${SOURCE[e.source] ?? e.source}` : "";
  switch (e.outcome) {
    case "updated":
      return `${e.from} → ${e.to ?? "?"}${source} · ${how}`;
    case "rolledBack":
      return `Rolled back ${e.from} → ${e.to ?? "?"}${e.message ? ` · ${e.message}` : ""}`;
    case "handedOff":
      return `${e.from} → ${e.to ?? "?"} · ${e.message ?? "finish elsewhere"}`;
    case "failed":
      return e.message ?? "Failed";
  }
}

function Glyph({ outcome }: { outcome: HistoryEntry["outcome"] }) {
  if (outcome === "updated") return <CheckCircle className="i s-done" />;
  if (outcome === "failed") return <Warning className="i s-failed" />;
  if (outcome === "rolledBack") return <Undo className="i s-handoff" />;
  return <External className="i s-handoff" />;
}

/** Everything Camu changed in the last 30 days, with Retry and Roll back. */
export function HistoryView({ entries, rows, backups, onRetry, onRollBack }: Props) {
  if (entries.length === 0) {
    return (
      <div className="empty">
        <p>No updates yet</p>
        <p className="faint">Updates, failures and rollbacks from the last 30 days show here.</p>
      </div>
    );
  }
  const byDay = new Map<string, HistoryEntry[]>();
  for (const e of entries) {
    const key = dayLabel(e.at);
    byDay.set(key, [...(byDay.get(key) ?? []), e]);
  }
  // Only the newest entry per app can be acted on.
  const newestPerApp = new Map<string, number>();
  for (const e of entries) if (!newestPerApp.has(e.bundleId)) newestPerApp.set(e.bundleId, e.at);

  return (
    <div className="list-wrap history">
      {[...byDay].map(([day, list]) => (
        <div key={day} className="group">
          <div className="group-head">
            <span>{day}</span>
            <span className="group-count">{list.length}</span>
          </div>
          <ul className="list">
            {list.map((e) => {
              const row = rows.find((r) => r.app.bundleId === e.bundleId);
              const newest = newestPerApp.get(e.bundleId) === e.at;
              const canRetry =
                newest && e.outcome === "failed" && row?.report?.status === "updateAvailable" && !row.report.hold;
              const canRollBack =
                newest &&
                e.outcome === "updated" &&
                row?.app.version === e.to &&
                backups.some((b) => b.bundleId === e.bundleId && b.newest);
              return (
                <li key={`${e.at}-${e.id}-${e.outcome}`} className="row history-row">
                  <Glyph outcome={e.outcome} />
                  {row ? <AppIcon app={row.app} /> : <span className="icon monogram" aria-hidden />}
                  <span className="row-name">{e.name}</span>
                  <span className={`history-what${e.outcome === "failed" ? " danger" : ""}`}>{describe(e)}</span>
                  <span className="row-fill" />
                  <span className="faint history-time">{timeLabel(e.at)}</span>
                  {canRetry && row ? (
                    <button type="button" className="btn go small" onClick={() => onRetry(row)}>
                      Retry
                    </button>
                  ) : canRollBack && row ? (
                    <button type="button" className="btn small" onClick={() => onRollBack(row)}>
                      Roll back
                    </button>
                  ) : (
                    <span className="history-spacer" />
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </div>
  );
}
