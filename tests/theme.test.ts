import { describe, expect, it } from "vitest";
import { clamp, resolveTheme, shapeRadius } from "../src/lib/theme";

describe("theme helpers", () => {
  it("resolves the automatic theme from the system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
  });
  it("maps shapes to radii", () => {
    expect(shapeRadius("circle")).toBe("50%");
    expect(shapeRadius("pill")).toBe("999px");
  });
  it("clamps and survives NaN", () => {
    expect(clamp(5, 0, 3)).toBe(3);
    expect(clamp(NaN, 1, 3)).toBe(1);
  });
});
