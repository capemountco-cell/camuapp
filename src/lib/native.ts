/**
 * Every call from the UI to Rust goes through this file, so the command
 * surface is visible in one place and argument shapes cannot drift.
 */

import { invoke } from "@tauri-apps/api/core";

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

export const greet = (name: string) => call<string>("greet", { name });
