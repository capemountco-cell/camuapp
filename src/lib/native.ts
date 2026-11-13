/**
 * Every call from the UI to Rust goes through this file, so the command
 * surface is visible in one place and argument shapes cannot drift.
 *
 * The types mirror the core crate's serialised structs (camelCase).
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Source = "sparkle" | "electron" | "appStore" | "github" | "homebrew";
export type Status = "updateAvailable" | "upToDate" | "unsupported" | "failed";

export interface InstalledApp {
  path: string;
  fileName: string;
  name: string;
  bundleId: string;
  version: string;
  build: string | null;
  sparkleFeed: string | null;
  fromAppStore: boolean;
  minimumOs: string | null;
  iconFile: string | null;
  executable?: string | null;
  /** The chips the app's binary runs on natively; empty if unreadable. */
  archs?: Arch[];
  setapp?: boolean;
}

export type Arch = "arm64" | "x86_64";

export interface Finding {
  source: Source;
  version: string;
  build: string | null;
  isUpdate: boolean;
  releaseNotes: string | null;
  releaseNotesUrl: string | null;
  downloadUrl: string | null;
  pageUrl: string | null;
  minimumOs: string | null;
  date: string | null;
  cask: string | null;
  signature?: string | null;
}

export type Install =
  | { kind: "homebrew"; cask: string }
  | { kind: "appStore"; url: string }
  | { kind: "download"; url: string }
  | { kind: "openApp" }
  | { kind: "website"; url: string }
  | { kind: "vendor"; name: string; path: string }
  | { kind: "none" };

/** Something to know before updating (mirrors camu_core::check::Notice). */
export type Notice =
  | { kind: "needsMacOs"; version: string }
  | { kind: "vendor"; name: string }
  | { kind: "setapp" };

/** Why an update is held back by the user's choice. */
export type Hold =
  | { kind: "skipped"; version: string }
  | { kind: "snoozed"; until: number }
  | { kind: "pinned"; major: number };

export interface Report {
  id: string;
  app: InstalledApp;
  status: Status;
  best: Finding | null;
  findings: Finding[];
  errors: { source: Source; message: string }[];
  install: Install;
  homebrewCask: string | null;
  /** The update moves to a new major version (5.x → 6.0). */
  major?: boolean;
  notice?: Notice | null;
  hold?: Hold | null;
}

/** Where an update is up to (mirrors camu_core::update::Phase). */
export type Phase =
  | { phase: "queued" }
  | { phase: "downloading"; received: number; total: number | null }
  | { phase: "verifying" }
  | { phase: "waiting"; message: string }
  | { phase: "installing" }
  | { phase: "done"; version: string }
  | { phase: "handoff"; message: string }
  | { phase: "failed"; message: string };

export type UpdateEvent = Phase & { id: string };

export interface Backup {
  bundleId: string;
  version: string;
  path: string;
  newest: boolean;
  made?: number;
}

export interface BackupSummary {
  totalBytes: number;
  deletedAppsBytes: number;
  count: number;
  folder: string;
}

/** One line of update history (mirrors camu_core::history::Entry). */
export interface HistoryEntry {
  at: number;
  id: string;
  bundleId: string;
  name: string;
  from: string;
  to: string | null;
  source: string | null;
  how: "automatic" | "manual";
  outcome: "updated" | "failed" | "handedOff" | "rolledBack";
  message: string | null;
}

/** True inside the Tauri window; false in a plain browser tab. */
export function nativeAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!nativeAvailable()) {
    throw new Error("The native side is not running. Start the app with `npm run dev`.");
  }
  return invoke<T>(command, args);
}

export const listApps = () => call<InstalledApp[]>("list_apps");

/**
 * Check every app (beta channels come from settings). Resolves with the
 * number checked. Progress arrives as events, also for scheduled checks.
 */
export const checkUpdates = () => call<number>("check_updates");

/** Calls `handler` for each report as `checkUpdates` produces it. */
export async function onReport(handler: (report: Report) => void): Promise<UnlistenFn> {
  if (!nativeAvailable()) return () => {};
  return listen<Report>("camu://report", (event) => handler(event.payload));
}

/** Update apps by id. Resolves when the whole run is over. */
export const updateApps = (ids: string[]) => call<void>("update_apps", { ids });

export async function onUpdate(handler: (event: UpdateEvent) => void): Promise<UnlistenFn> {
  if (!nativeAvailable()) return () => {};
  return listen<UpdateEvent>("camu://update", (event) => handler(event.payload));
}

/** Stored by the Rust side, so the menu bar and schedule can read them. */
export interface Settings {
  checkEveryHours: number;
  notify: boolean;
  autoUpdate: boolean;
  menuBar: boolean;
  showInDock: boolean;
  ignored: string[];
  beta: string[];
  skipped: Record<string, string>;
  snoozed: Record<string, number>;
  pinned: Record<string, number>;
  autoSkipOpen: boolean;
  autoNoPassword: boolean;
  askMajor: boolean;
  quietHours: { from: number; to: number } | null;
  backupDays: number;
  backupMaxGb: number;
}

export interface SelfUpdate {
  current: string;
  version: string;
  notes: string | null;
  pageUrl: string | null;
  downloadUrl: string | null;
  sha256: string | null;
  canInstall: boolean;
}

/** What the menu bar panel shows. */
export interface Snapshot {
  reports: Report[];
  lastCheck: number | null;
  checking: boolean;
  updating: boolean;
}

export const snapshot = () => call<Snapshot>("snapshot");
/** Show the main window, optionally on a view, and close the menu bar panel. */
export const showMain = (view?: string) => call<void>("show_main", { view: view ?? null });

/** True in the panel that opens under the menu bar icon. */
export function isPopover(): boolean {
  if (!nativeAvailable()) return new URLSearchParams(window.location.search).has("popover");
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } })
    .__TAURI_INTERNALS__;
  return internals?.metadata?.currentWindow?.label === "popover";
}

export const getSettings = () => call<Settings>("get_settings");
/** Resolves with the settings as stored, repaired if a combination made no sense. */
export const setSettings = (settings: Settings) => call<Settings>("set_settings", { settings });
export const appVersion = () => call<string>("app_version");
export const hostArch = () => call<Arch>("host_arch");
export const selfUpdateStatus = () => call<SelfUpdate | null>("self_update_status");
export const checkSelfUpdate = () => call<SelfUpdate | null>("check_self_update");
/** Installs and relaunches Camu; only resolves if it fails. */
export const installSelfUpdate = () => call<void>("install_self_update");

async function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!nativeAvailable()) return () => {};
  return listen<T>(event, (e) => handler(e.payload));
}

/** Lifecycle events, sent for checks and updates started anywhere. */
export const events = {
  checkStarted: (h: (apps: InstalledApp[]) => void) => on("camu://check-started", h),
  checkFinished: (h: (secs: number) => void) => on("camu://check-finished", h),
  updateStarted: (h: (ids: string[]) => void) => on("camu://update-started", h),
  updateFinished: (h: () => void) => on("camu://update-finished", h),
  selfUpdate: (h: (found: SelfUpdate | null) => void) => on("camu://self-update", h),
  /** The menu bar panel was opened. */
  popoverShown: (h: () => void) => on("camu://popover-shown", h),
  /** The menu bar panel asked the main window to show a view. */
  navigate: (h: (view: string) => void) => on("camu://navigate", h),
  /** The menu bar asked to show an app. */
  select: (h: (id: string) => void) => on("camu://select", h),
  /** Something was added to the update history. */
  historyChanged: (h: () => void) => on("camu://history-changed", h),
};

export type Choice = "skip" | "snooze" | "pin" | "clear";
/** Skip this version, snooze for `days`, stay on this major version, or clear. */
export const setChoice = (bundleId: string, choice: Choice, version?: string, days?: number) =>
  call<Settings>("set_choice", { bundleId, choice, version: version ?? null, days: days ?? null });
export const listHistory = () => call<HistoryEntry[]>("list_history");
export const backupSummary = () => call<BackupSummary>("backup_summary");
export const deleteBackups = (scope: "deleted" | "all") => call<number>("delete_backups", { scope });
export const revealBackups = () => call<void>("reveal_backups");

export const listBackups = () => call<Backup[]>("list_backups");

/** Restore an app's newest backup. Resolves with the restored version. */
export const rollBack = (path: string) => call<string>("roll_back", { path });

export const appIcon = (path: string, iconFile: string) =>
  call<string | null>("app_icon", { path, iconFile });

export const openUrl = (url: string) => call<void>("open_url", { url });

/** A wrong-version report, sent to camu.cc only when the user asks. */
export interface WrongVersion {
  app: string;
  bundleId: string;
  installed: string;
  found: string;
  source: string;
  url: string;
  note: string;
}
export const sendReport = (report: WrongVersion) => call<void>("send_report", { report });
export const openApp = (path: string) => call<void>("open_app", { path });
export const revealApp = (path: string) => call<void>("reveal_app", { path });
