/**
 * What the list shows, decided from reports and preferences. Pure functions,
 * so they are tested without a window.
 */

import type { Arch, Hold, InstalledApp, Install, Notice, Phase, Report, Source, WrongVersion } from "./native";

export type Tab = "updates" | "current" | "other" | "ignored";

/**
 * Ignore and beta lists from before settings moved to the Rust side. Read
 * once, merged into the stored settings, then cleared.
 */
export interface Prefs {
  /** Bundle ids whose updates are hidden. */
  ignored: string[];
  /** Bundle ids that follow beta channels. */
  beta: string[];
}

export const defaultPrefs: Prefs = { ignored: [], beta: [] };

export function clearPrefs(storage: Pick<Storage, "removeItem"> | undefined = globalThis.localStorage) {
  try {
    storage?.removeItem(PREFS_KEY);
  } catch {
    // Nothing to clear.
  }
}

/** Merge legacy preferences into settings; `null` when there is nothing to add. */
export function mergeLegacy<T extends Prefs>(settings: T, legacy: Prefs): T | null {
  const ignored = [...new Set([...settings.ignored, ...legacy.ignored])];
  const beta = [...new Set([...settings.beta, ...legacy.beta])];
  if (ignored.length === settings.ignored.length && beta.length === settings.beta.length) return null;
  return { ...settings, ignored, beta };
}

export const CHECK_INTERVALS: { hours: number; label: string }[] = [
  { hours: 0, label: "Never" },
  { hours: 1, label: "Every hour" },
  { hours: 6, label: "Every 6 hours" },
  { hours: 24, label: "Every day" },
  { hours: 168, label: "Every week" },
];

const PREFS_KEY = "camu.prefs.v1";

export function loadPrefs(storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage): Prefs {
  try {
    const raw = storage?.getItem(PREFS_KEY);
    if (!raw) return defaultPrefs;
    const parsed = JSON.parse(raw) as Partial<Prefs>;
    return {
      ignored: Array.isArray(parsed.ignored) ? parsed.ignored.filter((s) => typeof s === "string") : [],
      beta: Array.isArray(parsed.beta) ? parsed.beta.filter((s) => typeof s === "string") : [],
    };
  } catch {
    return defaultPrefs;
  }
}

export function savePrefs(prefs: Prefs, storage: Pick<Storage, "setItem"> | undefined = globalThis.localStorage) {
  try {
    storage?.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // Storage can be unavailable; preferences then last for this session.
  }
}

export function toggle(list: string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
}

/** One row: the app, plus its report once the check has reached it. */
export interface Row {
  app: InstalledApp;
  report?: Report;
}

export function tabOf(row: Row, ignored: string[]): Tab {
  if (ignored.includes(row.app.bundleId)) return "ignored";
  if (row.report?.notice?.kind === "setapp") return "other";
  switch (row.report?.status) {
    case "updateAvailable":
      return "updates";
    case "upToDate":
      return "current";
    default:
      return "other";
  }
}

export function group(rows: Row[], ignored: string[]): Record<Tab, Row[]> {
  const out: Record<Tab, Row[]> = { updates: [], current: [], other: [], ignored: [] };
  for (const row of rows) out[tabOf(row, ignored)].push(row);
  return out;
}

/** Narrow a list to apps whose answer came from one update source. */
export type SourceFilter = Source | "all";

export function bySource(row: Row, filter: SourceFilter): boolean {
  return filter === "all" || row.report?.best?.source === filter;
}

/** How many rows each source filter would leave. */
export function sourceCounts(rows: Row[]): Record<SourceFilter, number> {
  const counts: Record<SourceFilter, number> = { all: rows.length, homebrew: 0, sparkle: 0, appStore: 0, electron: 0, github: 0 };
  for (const row of rows) {
    const s = row.report?.best?.source;
    if (s) counts[s] += 1;
  }
  return counts;
}

export function matches(row: Row, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return row.app.name.toLowerCase().includes(q) || row.app.bundleId.toLowerCase().includes(q);
}

export const sourceLabel: Record<Source, string> = {
  sparkle: "Sparkle",
  electron: "Electron",
  appStore: "App Store",
  github: "GitHub",
  homebrew: "Homebrew",
};

/** How an update installs, in the words the detail panel uses. */
export function installLabel(install: Install): string {
  switch (install.kind) {
    case "homebrew":
      return `Homebrew (${install.cask})`;
    case "download":
      return "Download and replace";
    case "appStore":
      return "App Store";
    case "openApp":
      return "The app's own updater";
    case "website":
      return "Developer's website";
    case "vendor":
      return install.name;
    case "none":
      return "—";
  }
}

/** Camu installs these itself; the rest hand off to the App Store or the app. */
export function isOneClick(install: Install): boolean {
  return install.kind === "homebrew" || install.kind === "download";
}

export type UpdateGroup = "updated" | "oneClick" | "appStore" | "manual" | "needsYou" | "held";

export const updateGroupLabel: Record<UpdateGroup, string> = {
  updated: "Updated",
  oneClick: "Ready to install",
  appStore: "App Store",
  manual: "Finish in the app",
  needsYou: "Needs you",
  held: "Skipped",
};

/** An update that waits for the user: a major version (when they asked to
 *  be asked), one that needs a newer macOS, or the developer's updater. */
export function needsYou(report: Report | undefined, askMajor = true): boolean {
  if (!report || report.status !== "updateAvailable" || report.hold) return false;
  return Boolean(report.notice) || (askMajor && Boolean(report.major));
}

/** Group the Updates view: what Camu installs itself first. */
export function updateGroups(rows: Row[], phases: Map<string, Phase>, askMajor = true): [UpdateGroup, Row[]][] {
  const groups: Record<UpdateGroup, Row[]> = { updated: [], oneClick: [], appStore: [], manual: [], needsYou: [], held: [] };
  for (const row of rows) {
    const phase = phases.get(row.app.path);
    const install = row.report?.install;
    if (phase?.phase === "done") groups.updated.push(row);
    else if (row.report?.hold) groups.held.push(row);
    else if (needsYou(row.report, askMajor)) groups.needsYou.push(row);
    else if (install && isOneClick(install)) groups.oneClick.push(row);
    else if (install?.kind === "appStore") groups.appStore.push(row);
    else groups.manual.push(row);
  }
  return (["oneClick", "appStore", "manual", "needsYou", "held", "updated"] as UpdateGroup[])
    .filter((g) => groups[g].length > 0)
    .map((g) => [g, groups[g]]);
}

/** Rows Update All installs: not held back, not waiting for the user. */
export function updateAllRows(rows: Row[], askMajor = true): Row[] {
  return rows.filter((r) => r.report?.status === "updateAvailable" && !r.report.hold && !needsYou(r.report, askMajor));
}

/** A short badge for a notice. */
export function noticeLabel(notice: Notice): string {
  switch (notice.kind) {
    case "needsMacOs":
      return `Needs macOS ${notice.version}`;
    case "vendor":
      return `Updates through ${notice.name}`;
    case "setapp":
      return "Managed by Setapp";
  }
}

/** Why an update is held back, in a few words. */
export function holdLabel(hold: Hold, nowMs = Date.now()): string {
  switch (hold.kind) {
    case "skipped":
      return `${hold.version} skipped`;
    case "snoozed": {
      const days = Math.max(1, Math.ceil((hold.until * 1000 - nowMs) / 86_400_000));
      return `Reminding you in ${days} day${days === 1 ? "" : "s"}`;
    }
    case "pinned":
      return `Staying on ${hold.major}.x`;
  }
}

/** Built for Intel only, on an Apple silicon Mac: runs through Rosetta. */
export function intelOnly(app: InstalledApp, host: Arch | null): boolean {
  return host === "arm64" && app.archs?.length === 1 && app.archs[0] === "x86_64";
}

/** When most checks failed, the list can't be trusted: probably offline. */
export function checkTrouble(rows: Row[]): { failed: number; checked: number } | null {
  const checked = rows.filter((r) => r.report).length;
  const failed = rows.filter((r) => r.report?.status === "failed").length;
  return checked > 0 && failed > 0 && failed / checked >= 0.5 ? { failed, checked } : null;
}

/** Plain text describing what Camu saw, for reporting a wrong version. */
export function wrongVersionReport(row: Row, camuVersion: string | null): string {
  const { app, report } = row;
  const lines = [
    `Camu ${camuVersion ?? "?"}: wrong version report`,
    `App: ${app.name} (${app.bundleId})`,
    `Installed: ${app.version}${app.build && app.build !== app.version ? ` (build ${app.build})` : ""}`,
  ];
  for (const f of report?.findings ?? []) {
    lines.push(`${sourceLabel[f.source]}: ${f.version}${f.build ? ` (build ${f.build})` : ""}${f.isUpdate ? " — called an update" : ""}`);
  }
  for (const e of report?.errors ?? []) lines.push(`${sourceLabel[e.source]}: no answer (${e.message})`);
  return lines.join("\n");
}

/** The report sent to camu.cc: what the user has, and what Camu found. */
export function wrongVersionPayload(row: Row): WrongVersion {
  const { app, report } = row;
  const findings = report?.findings ?? [];
  const best = report?.best ?? findings.find((f) => f.isUpdate) ?? findings[0];
  const withBuild = (version: string, build: string | null | undefined) =>
    build && build !== version ? `${version} (build ${build})` : version;
  const lines = wrongVersionReport(row, null).split("\n").slice(3);
  return {
    app: app.name,
    bundleId: app.bundleId,
    installed: withBuild(app.version, app.build),
    found: best ? withBuild(best.version, best.build) : "",
    source: best?.source ?? "",
    url: best?.pageUrl ?? best?.releaseNotesUrl ?? best?.downloadUrl ?? "",
    note: lines.join(" · "),
  };
}

/** A phase that is still moving. */
export function isBusy(phase: Phase | undefined): boolean {
  return (
    phase !== undefined &&
    ["queued", "downloading", "verifying", "waiting", "installing"].includes(phase.phase)
  );
}

function megabytes(bytes: number): string {
  return `${(bytes / 1_048_576).toFixed(bytes < 10_485_760 ? 1 : 0)} MB`;
}

export function phaseLabel(phase: Phase): string {
  switch (phase.phase) {
    case "queued":
      return "Queued";
    case "downloading":
      return phase.total
        ? `Downloading ${Math.floor((phase.received / phase.total) * 100)}%`
        : phase.received > 0
          ? `Downloading ${megabytes(phase.received)}`
          : "Downloading";
    case "verifying":
      return "Verifying signature";
    case "waiting":
      return phase.message;
    case "installing":
      return "Installing";
    case "done":
      return `Updated to ${phase.version}`;
    case "handoff":
      return phase.message;
    case "failed":
      return phase.message;
  }
}

/** Progress from 0 to 1 for the row's status ring, when there is one. */
export function phaseProgress(phase: Phase): number | null {
  if (phase.phase === "downloading" && phase.total) return phase.received / phase.total;
  if (phase.phase === "verifying") return 1;
  return null;
}

/**
 * Loose matching for the command palette: every character of the query
 * appears in order. Returns a score (lower is better) or null for no match.
 */
export function fuzzy(text: string, query: string): number | null {
  const t = text.toLowerCase();
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const direct = t.indexOf(q);
  if (direct >= 0) return direct === 0 ? 0 : 1 + direct / 100;
  let at = 0;
  let gaps = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, at);
    if (found < 0) return null;
    gaps += found - at;
    at = found + 1;
  }
  return 10 + gaps;
}

/** What the "other" tab says about a row. */
export function otherReason(row: Row): string {
  if (!row.report) return "Checking…";
  if (row.report.notice?.kind === "setapp") return "Managed by Setapp";
  if (row.report.status === "failed") {
    const first = row.report.errors[0];
    return first ? `${sourceLabel[first.source]} didn't answer` : "Check failed";
  }
  return "No update source found";
}
