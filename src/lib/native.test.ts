import { describe, expect, it } from "vitest";

import { checkUpdates, listApps, nativeAvailable, onReport } from "./native";

describe("native bridge", () => {
  it("reports that it is not inside the Tauri window under test", () => {
    expect(nativeAvailable()).toBe(false);
  });

  it("refuses with a useful message instead of hanging", async () => {
    await expect(listApps()).rejects.toThrow(/npm run dev/);
    await expect(checkUpdates([])).rejects.toThrow(/npm run dev/);
  });

  it("subscribing outside the window is a harmless no-op", async () => {
    const unlisten = await onReport(() => {});
    expect(typeof unlisten).toBe("function");
  });
});
