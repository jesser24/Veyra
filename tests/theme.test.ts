import { describe, expect, it } from "vitest";
import { clamp, resolveTheme, tabRadius } from "../src/lib/theme";

describe("theme helpers", () => {
  it("resolves the automatic theme from the system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
  });
  it("rounds a tab only on the side facing the screen interior", () => {
    expect(tabRadius("tab", "left", 10)).toBe("0 12px 12px 0");
    expect(tabRadius("tab", "right", 10)).toBe("12px 0 0 12px");
    expect(tabRadius("tab", "top", 10)).toBe("0 0 12px 12px");
    expect(tabRadius("tab", "bottom", 10)).toBe("12px 12px 0 0");
  });
  it("uses a full pill when floating or when the pill shape is chosen", () => {
    expect(tabRadius("tab", null, 10)).toBe("999px");
    expect(tabRadius("pill", "left", 10)).toBe("999px");
  });
  it("clamps and survives NaN", () => {
    expect(clamp(5, 0, 3)).toBe(3);
    expect(clamp(NaN, 1, 3)).toBe(1);
  });
});
