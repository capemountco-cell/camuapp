/**
 * What the list shows, decided from reports and preferences. Pure functions,
 * so they are tested without a window.
 */

import type { InstalledApp, Install, Report, Source } from "./native";

export type Tab = "updates" | "current" | "other" | "ignored";
export type Theme = "black" | "light" | "system";

export interface Prefs {
  theme: Theme;
  /** Bundle ids whose updates are hidden. */
  ignored: string[];
  /** Bundle ids that follow beta channels. */
  beta: string[];
}

export const defaultPrefs: Prefs = { theme: "black", ignored: [], beta: [] };

const PREFS_KEY = "camu.prefs.v1";

export function loadPrefs(storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage): Prefs {
  try {
    const raw = storage?.getItem(PREFS_KEY);
    if (!raw) return defaultPrefs;
    const parsed = JSON.parse(raw) as Partial<Prefs>;
    return {
      theme: parsed.theme === "light" || parsed.theme === "system" ? parsed.theme : "black",
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

/** The button on a row with an update. */
export function actionLabel(install: Install): string | null {
  switch (install.kind) {
    case "appStore":
      return "App Store";
    case "openApp":
    case "homebrew":
    case "download":
      return "Open";
    case "website":
      return "Website";
    case "none":
      return null;
  }
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
