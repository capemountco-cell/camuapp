import { describe, expect, it } from "vitest";

import {
  defaultPrefs,
  fuzzy,
  group,
  loadPrefs,
  matches,
  mergeLegacy,
  phaseLabel,
  savePrefs,
  toggle,
  updateGroups,
  type Row,
} from "./model";
import type { Phase } from "./native";
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

describe("updates", () => {
  const withInstall = (name: string, install: Report["install"]): Row => {
    const a = app(name);
    return { app: a, report: { ...report(a, "updateAvailable"), install } };
  };

  it("puts what Camu installs itself first and finished ones last", () => {
    const rows = [
      withInstall("Store", { kind: "appStore", url: "macappstore://x" }),
      withInstall("Brew", { kind: "homebrew", cask: "brew" }),
      withInstall("Own", { kind: "openApp" }),
      withInstall("Done", { kind: "download", url: "https://x" }),
    ];
    const phases = new Map<string, Phase>([["/Applications/Done.app", { phase: "done", version: "2.0" }]]);
    const groups = updateGroups(rows, phases).map(([g, r]) => [g, r.map((x) => x.app.name)]);
    expect(groups).toEqual([
      ["oneClick", ["Brew"]],
      ["appStore", ["Store"]],
      ["manual", ["Own"]],
      ["updated", ["Done"]],
    ]);
  });

  it("describes progress in a few words", () => {
    expect(phaseLabel({ phase: "downloading", received: 50, total: 200 })).toBe("Downloading 25%");
    expect(phaseLabel({ phase: "downloading", received: 3_145_728, total: null })).toBe("Downloading 3.0 MB");
    expect(phaseLabel({ phase: "done", version: "2.0" })).toBe("Updated to 2.0");
  });
});

describe("command palette matching", () => {
  it("prefers prefixes, then substrings, then scattered letters", () => {
    expect(fuzzy("Update all", "upd")).toBe(0);
    expect(fuzzy("Check for updates", "updates")!).toBeLessThan(10);
    expect(fuzzy("Visual Studio Code", "vsc")!).toBeGreaterThanOrEqual(10);
    expect(fuzzy("Arc", "xyz")).toBeNull();
  });
});

describe("settings migration", () => {
  it("merges old ignore and beta lists once", () => {
    const stored = { ignored: ["a"], beta: [], notify: true };
    expect(mergeLegacy(stored, { ignored: ["a", "b"], beta: ["c"] })).toEqual({
      ignored: ["a", "b"],
      beta: ["c"],
      notify: true,
    });
    expect(mergeLegacy(stored, { ignored: ["a"], beta: [] })).toBeNull();
  });
});
