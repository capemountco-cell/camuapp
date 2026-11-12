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
}

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
}

export type Install =
  | { kind: "homebrew"; cask: string }
  | { kind: "appStore"; url: string }
  | { kind: "download"; url: string }
  | { kind: "openApp" }
  | { kind: "website"; url: string }
  | { kind: "none" };

export interface Report {
  id: string;
  app: InstalledApp;
  status: Status;
  best: Finding | null;
  findings: Finding[];
  errors: { source: Source; message: string }[];
  install: Install;
  homebrewCask: string | null;
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

/** Resolves with the number of apps checked once every report has arrived. */
export const checkUpdates = (includeBeta: string[]) => call<number>("check_updates", { includeBeta });

/** Calls `handler` for each report as `checkUpdates` produces it. */
export async function onReport(handler: (report: Report) => void): Promise<UnlistenFn> {
  if (!nativeAvailable()) return () => {};
  return listen<Report>("camu://report", (event) => handler(event.payload));
}

export const appIcon = (path: string, iconFile: string) =>
  call<string | null>("app_icon", { path, iconFile });

export const openUrl = (url: string) => call<void>("open_url", { url });
export const openApp = (path: string) => call<void>("open_app", { path });
export const revealApp = (path: string) => call<void>("reveal_app", { path });
