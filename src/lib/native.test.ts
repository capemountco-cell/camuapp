import { describe, expect, it } from "vitest";

import { greet, nativeAvailable } from "./native";

describe("native bridge", () => {
  it("reports that it is not inside the Tauri window under test", () => {
    expect(nativeAvailable()).toBe(false);
  });

  it("refuses with a useful message instead of hanging", async () => {
    await expect(greet("you")).rejects.toThrow(/npm run dev/);
  });
});
