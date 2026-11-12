import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { CommandPalette, type PaletteCommand } from "./components/CommandPalette";
import { DetailPanel } from "./components/DetailPanel";
import { ArrowUpCircle, CheckCircle, EyeOff, Folder, Question, Refresh, Undo } from "./components/Icons";
import { Kbd } from "./components/Kbd";
import { Row } from "./components/Row";
import { SettingsView } from "./components/SettingsView";
import { TopBar, type View } from "./components/TopBar";
import {
  group,
  isBusy,
  isOneClick,
  clearPrefs,
  loadPrefs,
  matches,
  mergeLegacy,
  toggle,
  updateGroupLabel,
  updateGroups,
  type Row as RowData,
  type Tab,
} from "./lib/model";
import {
  appVersion,
  checkSelfUpdate,
  checkUpdates,
  events,
  getSettings,
  installSelfUpdate,
  listBackups,
  selfUpdateStatus,
  setSettings,
  nativeAvailable,
  onReport,
  onUpdate,
  openUrl,
  revealApp,
  rollBack,
  updateApps,
  type Backup,
  type SelfUpdate,
  type Settings,
  type InstalledApp,
  type Phase,
  type Report,
} from "./lib/native";

const TAB_TITLE: Record<View, string> = {
  updates: "Updates",
  current: "Up to date",
  other: "Unsupported",
  ignored: "Ignored",
  settings: "Settings",
};

const NO_SETTINGS: Settings = {
  checkEveryHours: 6,
  notify: true,
  autoUpdate: true,
  menuBar: true,
  showInDock: true,
  ignored: [],
  beta: [],
};

const GO_KEY: Record<Tab, string> = { updates: "U", current: "C", other: "S", ignored: "I" };

function timeAgo(date: Date, now: number): string {
  const minutes = Math.round((now - date.getTime()) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours}h ago` : date.toLocaleDateString();
}

/** "in 5h 12m", "in 3m", "soon". */
function untilLabel(ms: number): string {
  if (ms <= 60_000) return "soon";
  const minutes = Math.round(ms / 60_000);
  if (minutes < 60) return `in ${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `in ${hours}h ${minutes % 60}m` : `in ${Math.round(hours / 24)}d`;
}

interface Toast {
  id: number;
  text: string;
  tone: "ok" | "error" | "info";
}

export function App() {
  const [prefs, setPrefsState] = useState<Settings>(NO_SETTINGS);
  const [version, setVersion] = useState<string | null>(null);
  const [selfUpdate, setSelfUpdate] = useState<SelfUpdate | null>(null);
  const [checkingSelf, setCheckingSelf] = useState(false);
  const [apps, setApps] = useState<InstalledApp[] | null>(nativeAvailable() ? null : []);
  const [reports, setReports] = useState<Map<string, Report>>(new Map());
  const [phases, setPhases] = useState<Map<string, Phase>>(new Map());
  const [backups, setBackups] = useState<Backup[]>([]);
  const [checking, setChecking] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [lastChecked, setLastChecked] = useState<Date | null>(null);
  const [view, setView] = useState<View>("updates");
  const tab: Tab = view === "settings" ? "updates" : view;
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [palette, setPalette] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [now, setNow] = useState(() => Date.now());
  const filterInput = useRef<HTMLInputElement>(null);
  const rowRefs = useRef(new Map<string, HTMLLIElement>());
  const pendingG = useRef<number | null>(null);

  const toast = useCallback((text: string, tone: Toast["tone"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-2), { id, text, tone }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 5000);
  }, []);

  const saveSettings = useCallback(
    (next: Settings) => {
      setPrefsState(next);
      setSettings(next).then(setPrefsState, (c) => toast(String(c), "error"));
    },
    [toast],
  );
  const updatePrefs = useCallback(
    (change: (p: Settings) => Settings) => setPrefsState((current) => {
      const next = change(current);
      setSettings(next).then(setPrefsState, (c) => toast(String(c), "error"));
      return next;
    }),
    [toast],
  );

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);

  const refreshBackups = useCallback(() => {
    if (nativeAvailable()) listBackups().then(setBackups, () => {});
  }, []);

  // State follows events, so checks and updates started from the menu bar
  // or the schedule show up here exactly like ones started in the window.
  const check = useCallback(() => {
    if (checking || updating) return;
    checkUpdates().catch((cause) => {
      if (!String(cause).includes("already running")) toast(String(cause), "error");
    });
  }, [checking, updating, toast]);

  useEffect(() => {
    const subscriptions = [
      events.checkStarted((list) => {
        setApps(list);
        setReports(new Map());
        setPhases(new Map());
        setChecking(true);
      }),
      onReport((report) => setReports((m) => new Map(m).set(report.id, report))),
      events.checkFinished((secs) => {
        setChecking(false);
        setLastChecked(new Date(secs * 1000));
      }),
      events.updateStarted((ids) => {
        setUpdating(true);
        setPhases((m) => {
          const next = new Map(m);
          ids.forEach((id) => next.set(id, { phase: "queued" }));
          return next;
        });
      }),
      onUpdate(({ id, ...phase }) => setPhases((m) => new Map(m).set(id, phase as Phase))),
      events.updateFinished(() => {
        setUpdating(false);
        refreshBackups();
      }),
      events.selfUpdate(setSelfUpdate),
      events.select((id) => {
        setSelected(id);
        setPanelOpen(true);
        setView("updates");
      }),
    ];
    return () => subscriptions.forEach((p) => void p.then((stop) => stop()));
  }, [refreshBackups]);

  const started = useRef(false);
  useEffect(() => {
    if (started.current || !nativeAvailable()) return;
    started.current = true;
    refreshBackups();
    appVersion().then(setVersion, () => {});
    selfUpdateStatus().then(setSelfUpdate, () => {});
    // Settings live on the Rust side now; bring over choices made before.
    getSettings()
      .then(async (stored) => {
        const merged = mergeLegacy(stored, loadPrefs());
        setPrefsState(merged ? await setSettings(merged) : stored);
        clearPrefs();
      })
      .catch(() => {})
      .finally(() => check());
  }, [check, refreshBackups]);

  const rows: RowData[] = useMemo(
    () => (apps ?? []).map((app) => ({ app, report: reports.get(app.path) })),
    [apps, reports],
  );
  const tabs = useMemo(() => group(rows, prefs.ignored), [rows, prefs.ignored]);
  const visible = useMemo(() => tabs[tab].filter((r) => matches(r, filter)), [tabs, tab, filter]);
  const groups = useMemo(
    () =>
      tab === "updates"
        ? updateGroups(visible, phases).map(([g, r]) => [updateGroupLabel[g], r] as const)
        : [[null, visible] as const],
    [tab, visible, phases],
  );
  const ordered = useMemo(() => groups.flatMap(([, r]) => r), [groups]);
  const selectedRow = ordered.find((r) => r.app.path === selected) ?? null;

  /** Updates that can start now: not ignored, not finished, not running. */
  const pending = useMemo(
    () =>
      tabs.updates.filter((r) => {
        const p = phases.get(r.app.path);
        return p?.phase !== "done" && !isBusy(p);
      }),
    [tabs.updates, phases],
  );
  const oneClick = pending.filter((r) => r.report && isOneClick(r.report.install));
  const updateCount = tabs.updates.filter((r) => phases.get(r.app.path)?.phase !== "done").length;

  const runUpdates = useCallback(
    (targets: RowData[]) => {
      if (updating || targets.length === 0) return;
      updateApps(targets.map((r) => r.app.path)).catch((cause) => toast(String(cause), "error"));
    },
    [updating, toast],
  );

  // Summarise a run once it is over.
  const runIds = useRef<string[]>([]);
  useEffect(() => {
    if (updating) {
      runIds.current = [...phases].filter(([, p]) => isBusy(p)).map(([id]) => id);
      return;
    }
    const ran = runIds.current;
    if (ran.length === 0) return;
    runIds.current = [];
    const results = ran.map((id) => phases.get(id)?.phase);
    const done = results.filter((p) => p === "done").length;
    const failed = results.filter((p) => p === "failed").length;
    const handed = results.filter((p) => p === "handoff").length;
    const parts = [
      done && `${done} updated`,
      handed && `${handed} to finish elsewhere`,
      failed && `${failed} failed`,
    ].filter(Boolean);
    if (parts.length) toast(parts.join(" · "), failed ? "error" : "ok");
  }, [updating, phases, toast]);

  const updateAll = useCallback(() => runUpdates(pending), [pending, runUpdates]);

  const act = {
    update: (row: RowData) => runUpdates([row]),
    ignore: (row: RowData) => updatePrefs((p) => ({ ...p, ignored: toggle(p.ignored, row.app.bundleId) })),
    beta: (row: RowData) => updatePrefs((p) => ({ ...p, beta: toggle(p.beta, row.app.bundleId) })),
    reveal: (row: RowData) => void revealApp(row.app.path).catch((c) => toast(String(c), "error")),
    link: (url: string) => void openUrl(url).catch((c) => toast(String(c), "error")),
    rollBack: (row: RowData) =>
      void rollBack(row.app.path).then(
        (version) => {
          toast(`${row.app.name} is back on ${version}`, "ok");
          refreshBackups();
        },
        (c) => toast(String(c), "error"),
      ),
  };

  const goTab = useCallback((next: View) => {
    setView(next);
    setSelected(null);
    setPanelOpen(false);
  }, []);

  const move = (delta: number) => {
    if (ordered.length === 0) return;
    const index = ordered.findIndex((r) => r.app.path === selected);
    const nextIndex = index < 0 ? 0 : Math.min(Math.max(index + delta, 0), ordered.length - 1);
    const next = ordered[nextIndex]!.app.path;
    setSelected(next);
    rowRefs.current.get(next)?.scrollIntoView({ block: "nearest" });
  };

  // The keyboard model: single keys act on the selected row.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      const key = e.key.toLowerCase();

      if (e.metaKey && key === "k") {
        e.preventDefault();
        setPalette((p) => !p);
        return;
      }
      if (palette) return;
      if (e.metaKey && key === ",") {
        e.preventDefault();
        goTab("settings");
        return;
      }
      if (e.metaKey && key === "r") {
        e.preventDefault();
        check();
        return;
      }
      if (e.metaKey && key === "u") {
        e.preventDefault();
        updateAll();
        return;
      }
      if (e.metaKey && key === "f") {
        e.preventDefault();
        filterInput.current?.focus();
        return;
      }
      if (e.key === "Escape") {
        if (typing) {
          setFilter("");
          (e.target as HTMLElement).blur();
        } else if (panelOpen) setPanelOpen(false);
        else setSelected(null);
        return;
      }
      if (typing || e.metaKey || e.ctrlKey || e.altKey) return;

      if (pendingG.current !== null) {
        window.clearTimeout(pendingG.current);
        pendingG.current = null;
        const target = (Object.keys(GO_KEY) as Tab[]).find((t) => GO_KEY[t].toLowerCase() === key);
        if (target) {
          e.preventDefault();
          goTab(target);
          return;
        }
      }
      if (key === "g") {
        pendingG.current = window.setTimeout(() => (pendingG.current = null), 800);
      } else if (key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        move(1);
      } else if (key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        move(-1);
      } else if (e.key === "Enter" || e.key === " ") {
        if (selectedRow) {
          e.preventDefault();
          setPanelOpen((o) => !o);
        }
      } else if (key === "/") {
        e.preventDefault();
        filterInput.current?.focus();
      } else if (key === "u" && selectedRow?.report?.status === "updateAvailable") {
        act.update(selectedRow);
      } else if (key === "i" && selectedRow) {
        act.ignore(selectedRow);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const commands: PaletteCommand[] = [];
  if (pending.length)
    commands.push({
      id: "update-all",
      group: "Actions",
      label: `Update all (${pending.length})`,
      keys: ["⌘", "U"],
      icon: <ArrowUpCircle />,
      run: updateAll,
    });
  commands.push({
    id: "check",
    group: "Actions",
    label: "Check for updates",
    keys: ["⌘", "R"],
    icon: <Refresh />,
    run: () => check(),
  });
  commands.push({
    id: "settings",
    group: "Go to",
    label: "Settings",
    keys: ["⌘", ","],
    run: () => goTab("settings"),
  });
  if (selectedRow) {
    const r = selectedRow;
    if (r.report?.status === "updateAvailable")
      commands.push({
        id: "update-one",
        group: "Actions",
        label: `Update ${r.app.name}`,
        keys: ["U"],
        icon: <ArrowUpCircle />,
        run: () => act.update(r),
      });
    commands.push({
      id: "ignore-one",
      group: "Actions",
      label: `${prefs.ignored.includes(r.app.bundleId) ? "Stop ignoring" : "Ignore"} ${r.app.name}`,
      keys: ["I"],
      icon: <EyeOff />,
      run: () => act.ignore(r),
    });
    commands.push({
      id: "reveal-one",
      group: "Actions",
      label: `Show ${r.app.name} in Finder`,
      icon: <Folder />,
      run: () => act.reveal(r),
    });
    if (backups.some((b) => b.bundleId === r.app.bundleId && b.newest))
      commands.push({
        id: "rollback-one",
        group: "Actions",
        label: `Roll back ${r.app.name}`,
        icon: <Undo />,
        run: () => act.rollBack(r),
      });
  }
  const viewIcons: Record<Tab, React.ReactNode> = {
    updates: <ArrowUpCircle />,
    current: <CheckCircle />,
    other: <Question />,
    ignored: <EyeOff />,
  };
  for (const id of Object.keys(viewIcons) as Tab[])
    commands.push({
      id: `go-${id}`,
      group: "Go to",
      label: TAB_TITLE[id],
      keys: ["G", GO_KEY[id]],
      icon: viewIcons[id],
      run: () => goTab(id),
    });
  for (const row of rows)
    commands.push({
      id: `app-${row.app.path}`,
      group: "Apps",
      label: row.app.name,
      run: () => {
        const target = (Object.keys(tabs) as Tab[]).find((t) => tabs[t].includes(row));
        if (target) setView(target);
        setFilter("");
        setSelected(row.app.path);
        setPanelOpen(true);
      },
    });

  const status = checking
    ? `Checking ${reports.size} of ${apps?.length ?? "…"}`
    : lastChecked
      ? `Last checked ${timeAgo(lastChecked, now)}`
      : nativeAvailable()
        ? "Not checked yet"
        : "Open Camu as an app to check";
  const nextCheck =
    !checking && lastChecked && prefs.checkEveryHours > 0
      ? untilLabel(lastChecked.getTime() + prefs.checkEveryHours * 3_600_000 - now)
      : null;

  const counts: Record<Tab, number> = {
    updates: updateCount,
    current: tabs.current.length,
    other: tabs.other.length,
    ignored: tabs.ignored.length,
  };

  return (
    <div className="shell">
      <TopBar
        view={view}
        counts={counts}
        checking={checking}
        selfUpdate={selfUpdate}
        onView={goTab}
        onCheck={check}
        onPalette={() => setPalette(true)}
      />

      <section className="main">
        <header className="header">
          <h1>{TAB_TITLE[view]}</h1>
          {view !== "settings" && <span className="header-count">{counts[tab]}</span>}
          <span className="spacer" />
          {view !== "settings" && (
          <input
            ref={filterInput}
            className="filter"
            type="search"
            placeholder="Filter"
            aria-label="Filter apps"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            spellCheck={false}
          />
          )}
          {view === "updates" && (
            <button
              type="button"
              className="btn primary"
              onClick={updateAll}
              disabled={updating || checking || pending.length === 0}
              title={
                oneClick.length < pending.length
                  ? `${oneClick.length} install here; ${pending.length - oneClick.length} finish in the App Store or the app`
                  : undefined
              }
            >
              {updating ? "Updating…" : pending.length ? `Update all ${pending.length}` : "Update all"}
              {!updating && pending.length > 0 && <Kbd keys={["⌘", "U"]} />}
            </button>
          )}
        </header>

        {view === "settings" ? (
          <SettingsView
            settings={prefs}
            version={version}
            selfUpdate={selfUpdate}
            checkingSelf={checkingSelf}
            onChange={saveSettings}
            onLink={act.link}
            onCheckSelf={() => {
              setCheckingSelf(true);
              checkSelfUpdate()
                .then((found) => {
                  setSelfUpdate(found);
                  if (!found) toast("Camu is up to date", "ok");
                })
                .catch((c) => toast(String(c), "error"))
                .finally(() => setCheckingSelf(false));
            }}
            onInstallSelf={() => {
              toast("Downloading Camu…");
              installSelfUpdate().catch((c) => toast(String(c), "error"));
            }}
          />
        ) : (
        <div className="content">
          <div className="list-wrap" role="listbox" aria-label={TAB_TITLE[tab]}>
            {apps === null ? (
              <ul className="list">
                {Array.from({ length: 9 }, (_, i) => (
                  <li key={i} className="row skeleton" style={{ opacity: 1 - i * 0.1 }}>
                    <span className="i skeleton-dot" />
                    <span className="sk sk-icon" />
                    <span className="sk" style={{ width: `${120 + ((i * 37) % 90)}px` }} />
                    <span className="row-fill" />
                    <span className="sk" style={{ width: 64 }} />
                  </li>
                ))}
              </ul>
            ) : visible.length === 0 ? (
              <div className="empty">
                {filter ? (
                  <p className="faint">No apps match “{filter}”</p>
                ) : tab === "updates" && checking ? (
                  <p className="faint">Looking for updates…</p>
                ) : tab === "updates" && lastChecked ? (
                  <>
                    <CheckCircle className="i empty-icon" />
                    <p>Everything is up to date</p>
                    <p className="faint">Camu checks every app against every source that knows it.</p>
                  </>
                ) : (
                  <p className="faint">Nothing here</p>
                )}
              </div>
            ) : (
              groups.map(([name, groupRows]) => (
                <div key={name ?? "all"} className="group">
                  {name && (
                    <div className="group-head">
                      <span>{name}</span>
                      <span className="group-count">{groupRows.length}</span>
                    </div>
                  )}
                  <ul className="list">
                    {groupRows.map((row) => (
                      <Row
                        key={row.app.path}
                        ref={(el) => {
                          if (el) rowRefs.current.set(row.app.path, el);
                          else rowRefs.current.delete(row.app.path);
                        }}
                        row={row}
                        phase={phases.get(row.app.path)}
                        selected={selected === row.app.path}
                        open={panelOpen && selected === row.app.path}
                        onSelect={() => setSelected(row.app.path)}
                        onOpen={() => {
                          setSelected(row.app.path);
                          setPanelOpen(true);
                        }}
                      />
                    ))}
                  </ul>
                </div>
              ))
            )}
          </div>

          {panelOpen && selectedRow && (
            <DetailPanel
              row={selectedRow}
              phase={phases.get(selectedRow.app.path)}
              backup={backups.find((b) => b.bundleId === selectedRow.app.bundleId && b.newest)}
              ignored={prefs.ignored.includes(selectedRow.app.bundleId)}
              beta={prefs.beta.includes(selectedRow.app.bundleId)}
              onClose={() => setPanelOpen(false)}
              onUpdate={() => act.update(selectedRow)}
              onIgnore={() => act.ignore(selectedRow)}
              onBeta={() => act.beta(selectedRow)}
              onReveal={() => act.reveal(selectedRow)}
              onLink={act.link}
              onRollBack={() => act.rollBack(selectedRow)}
            />
          )}
        </div>
        )}
        <footer className="statusbar">
          <span>
            {apps?.length ?? 0} apps
            {prefs.ignored.length > 0 && <> · {prefs.ignored.length} ignored</>}
            {counts.updates > 0 && (
              <>
                {" · "}
                <span className="accent">{counts.updates} with updates</span>
              </>
            )}
          </span>
          <span className="spacer" />
          <span>{status}</span>
          {nextCheck && <span className="faint">Next check {nextCheck}</span>}
          {prefs.autoUpdate && prefs.checkEveryHours > 0 && <span className="faint">Auto-install on</span>}
        </footer>
      </section>

      {palette && <CommandPalette commands={commands} onClose={() => setPalette(false)} />}

      <div className="toasts" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`toast t-${t.tone}`}>
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}
