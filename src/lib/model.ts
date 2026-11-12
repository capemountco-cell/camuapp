/**
 * What the list shows, decided from reports and preferences. Pure functions,
 * so they are tested without a window.
 */

import type { InstalledApp, Install, Phase, Report, Source } from "./native";

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
    case "none":
      return "—";
  }
}

/** Camu installs these itself; the rest hand off to the App Store or the app. */
export function isOneClick(install: Install): boolean {
  return install.kind === "homebrew" || install.kind === "download";
}

export type UpdateGroup = "updated" | "oneClick" | "appStore" | "manual";

export const updateGroupLabel: Record<UpdateGroup, string> = {
  updated: "Updated",
  oneClick: "Ready to install",
  appStore: "App Store",
  manual: "Finish in the app",
};

/** Group the Updates view: what Camu installs itself first. */
export function updateGroups(rows: Row[], phases: Map<string, Phase>): [UpdateGroup, Row[]][] {
  const groups: Record<UpdateGroup, Row[]> = { updated: [], oneClick: [], appStore: [], manual: [] };
  for (const row of rows) {
    const phase = phases.get(row.app.path);
    const install = row.report?.install;
    if (phase?.phase === "done") groups.updated.push(row);
    else if (install && isOneClick(install)) groups.oneClick.push(row);
    else if (install?.kind === "appStore") groups.appStore.push(row);
    else groups.manual.push(row);
  }
  return (["oneClick", "appStore", "manual", "updated"] as UpdateGroup[])
    .filter((g) => groups[g].length > 0)
    .map((g) => [g, groups[g]]);
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
  if (row.report.status === "failed") {
    const first = row.report.errors[0];
    return first ? `${sourceLabel[first.source]} didn't answer` : "Check failed";
  }
  return "No update source found";
}
