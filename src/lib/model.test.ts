import { describe, expect, it } from "vitest";

import { defaultPrefs, group, loadPrefs, matches, savePrefs, toggle, type Row } from "./model";
import type { InstalledApp, Report } from "./native";

const app = (name: string, bundleId = `com.test.${name.toLowerCase()}`): InstalledApp => ({
  path: `/Applications/${name}.app`,
  fileName: name,
  name,
  bundleId,
  version: "1.0",
  build: null,
  sparkleFeed: null,
  fromAppStore: false,
  minimumOs: null,
  iconFile: null,
});

const report = (a: InstalledApp, status: Report["status"]): Report => ({
  id: a.path,
  app: a,
  status,
  best: null,
  findings: [],
  errors: [],
  install: { kind: "none" },
  homebrewCask: null,
});

describe("grouping", () => {
  it("sorts rows into tabs, with ignored apps set aside", () => {
    const [a, b, c, d, e] = [app("A"), app("B"), app("C"), app("D"), app("E")] as const;
    const rows: Row[] = [
      { app: a, report: report(a, "updateAvailable") },
      { app: b, report: report(b, "upToDate") },
      { app: c, report: report(c, "unsupported") },
      { app: d },
      { app: e, report: report(e, "updateAvailable") },
    ];
    const tabs = group(rows, ["com.test.e"]);
    expect(tabs.updates.map((r) => r.app.name)).toEqual(["A"]);
    expect(tabs.current.map((r) => r.app.name)).toEqual(["B"]);
    expect(tabs.other.map((r) => r.app.name)).toEqual(["C", "D"]);
    expect(tabs.ignored.map((r) => r.app.name)).toEqual(["E"]);
  });

  it("searches names and bundle ids", () => {
    const row = { app: app("Visual Studio Code", "com.microsoft.VSCode") };
    expect(matches(row, "studio")).toBe(true);
    expect(matches(row, "microsoft")).toBe(true);
    expect(matches(row, "   ")).toBe(true);
    expect(matches(row, "xcode")).toBe(false);
  });
});

describe("preferences", () => {
  it("default to empty and survive a round trip", () => {
    const store = new Map<string, string>();
    const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
    expect(loadPrefs(storage)).toEqual(defaultPrefs);
    savePrefs({ ignored: ["x"], beta: [] }, storage);
    expect(loadPrefs(storage)).toEqual({ ignored: ["x"], beta: [] });
  });

  it("ignore corrupt storage", () => {
    expect(loadPrefs({ getItem: () => "{not json" })).toEqual(defaultPrefs);
    expect(loadPrefs({ getItem: () => '{"theme":"light","ignored":[1,"a"]}' })).toEqual({
      ignored: ["a"],
      beta: [],
    });
  });

  it("toggles ids in and out of a list", () => {
    expect(toggle([], "a")).toEqual(["a"]);
    expect(toggle(["a", "b"], "a")).toEqual(["b"]);
  });
});
