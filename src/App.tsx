import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { AppRow } from "./components/AppRow";
import { group, loadPrefs, matches, savePrefs, toggle, type Prefs, type Row, type Tab, type Theme } from "./lib/model";
import {
  checkUpdates,
  listApps,
  nativeAvailable,
  onReport,
  openApp,
  openUrl,
  revealApp,
  type Report,
} from "./lib/native";

const TABS: { id: Tab; label: string }[] = [
  { id: "updates", label: "Updates" },
  { id: "current", label: "Up to date" },
  { id: "other", label: "Unsupported" },
  { id: "ignored", label: "Ignored" },
];

const THEMES: Theme[] = ["black", "light", "system"];
const THEME_LABEL: Record<Theme, string> = { black: "Black", light: "Light", system: "System" };

function timeAgo(date: Date, now: number): string {
  const minutes = Math.round((now - date.getTime()) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours} h ago` : date.toLocaleDateString();
}

export function App() {
  const [prefs, setPrefs] = useState<Prefs>(() => loadPrefs());
  const [apps, setApps] = useState<Row["app"][]>([]);
  const [reports, setReports] = useState<Map<string, Report>>(new Map());
  const [checking, setChecking] = useState(false);
  const [lastChecked, setLastChecked] = useState<Date | null>(null);
  const [tab, setTab] = useState<Tab>("updates");
  const [query, setQuery] = useState("");
  const [openId, setOpenId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const search = useRef<HTMLInputElement>(null);

  const updatePrefs = useCallback((change: (p: Prefs) => Prefs) => {
    setPrefs((current) => {
      const next = change(current);
      savePrefs(next);
      return next;
    });
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = prefs.theme;
  }, [prefs.theme]);

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);

  const check = useCallback(async () => {
    if (checking) return;
    setChecking(true);
    setError(null);
    setReports(new Map());
    try {
      setApps(await listApps());
      await checkUpdates(prefs.beta);
      setLastChecked(new Date());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setChecking(false);
    }
  }, [checking, prefs.beta]);

  // Reports stream in as each app is checked.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onReport((report) => {
      setReports((current) => new Map(current).set(report.id, report));
    }).then((stop) => (unlisten = stop));
    return () => unlisten?.();
  }, []);

  // Check once on launch.
  const started = useRef(false);
  useEffect(() => {
    if (started.current || !nativeAvailable()) return;
    started.current = true;
    void check();
  }, [check]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "r") {
        e.preventDefault();
        void check();
      } else if (e.metaKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        search.current?.focus();
      } else if (e.key === "Escape") {
        if (document.activeElement === search.current && query) setQuery("");
        else setOpenId(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [check, query]);

  const rows: Row[] = useMemo(() => apps.map((app) => ({ app, report: reports.get(app.path) })), [apps, reports]);
  const tabs = useMemo(() => group(rows, prefs.ignored), [rows, prefs.ignored]);
  const visible = tabs[tab].filter((row) => matches(row, query));
  const done = reports.size;
  const total = apps.length;

  const act = (report: Report) => {
    const run = (() => {
      switch (report.install.kind) {
        case "appStore":
        case "website":
          return openUrl(report.install.url);
        default:
          return openApp(report.app.path);
      }
    })();
    run.catch((cause) => setError(String(cause)));
  };

  const summary = checking
    ? `Checking ${done} of ${total || "…"}`
    : !nativeAvailable()
      ? "Open Camu as an app to check for updates"
      : tabs.updates.length === 0
        ? lastChecked
          ? "Everything is up to date"
          : "Not checked yet"
        : `${tabs.updates.length} update${tabs.updates.length === 1 ? "" : "s"} available`;

  return (
    <div className="shell">
      <header className="toolbar">
        <div className="title">
          <h1>Camu</h1>
          <p className="summary">
            {summary}
            {!checking && lastChecked && <span className="muted"> · {timeAgo(lastChecked, now)}</span>}
          </p>
        </div>
        <div className="tools">
          <input
            ref={search}
            className="search"
            type="search"
            placeholder="Search"
            aria-label="Search apps"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            spellCheck={false}
          />
          <button
            type="button"
            className="ghost"
            title="Appearance"
            onClick={() =>
              updatePrefs((p) => ({ ...p, theme: THEMES[(THEMES.indexOf(p.theme) + 1) % THEMES.length] ?? "black" }))
            }
          >
            {THEME_LABEL[prefs.theme]}
          </button>
          <button type="button" className="primary" onClick={() => void check()} disabled={checking}>
            {checking ? "Checking…" : "Check"}
          </button>
        </div>
      </header>

      <div className="progress" aria-hidden>
        <div
          className="bar"
          style={{ width: checking && total ? `${(done / total) * 100}%` : "0%", opacity: checking ? 1 : 0 }}
        />
      </div>

      <nav className="tabs" role="tablist">
        {TABS.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? "tab active" : "tab"}
            onClick={() => setTab(id)}
          >
            {label}
            <span className="count">{tabs[id].length}</span>
          </button>
        ))}
      </nav>

      {error && (
        <p className="banner" role="alert">
          {error}
        </p>
      )}

      <main className="list-wrap">
        {visible.length === 0 ? (
          <div className="empty">
            {query ? (
              <p>No apps match “{query}”.</p>
            ) : tab === "updates" && !checking && lastChecked ? (
              <>
                <p className="big">All caught up.</p>
                <p className="muted">Every app Camu can check is on its newest version.</p>
              </>
            ) : tab === "updates" && checking ? (
              <p className="muted">Looking for updates…</p>
            ) : (
              <p className="muted">Nothing here.</p>
            )}
          </div>
        ) : (
          <ul className="list">
            {visible.map((row) => (
              <AppRow
                key={row.app.path}
                row={row}
                open={openId === row.app.path}
                ignored={prefs.ignored.includes(row.app.bundleId)}
                beta={prefs.beta.includes(row.app.bundleId)}
                onToggleOpen={() => setOpenId((id) => (id === row.app.path ? null : row.app.path))}
                onAction={act}
                onIgnore={() => updatePrefs((p) => ({ ...p, ignored: toggle(p.ignored, row.app.bundleId) }))}
                onBeta={() => updatePrefs((p) => ({ ...p, beta: toggle(p.beta, row.app.bundleId) }))}
                onReveal={() => void revealApp(row.app.path).catch((c) => setError(String(c)))}
                onLink={(url) => void openUrl(url).catch((c) => setError(String(c)))}
              />
            ))}
          </ul>
        )}
      </main>
    </div>
  );
}
