import type { Settings, Shape } from "./types";

export function resolveTheme(theme: Settings["theme"], systemDark: boolean): "light" | "dark" {
  return theme === "system" ? (systemDark ? "dark" : "light") : theme;
}
/** CSS border-radius for the toggle shape (the window is square; the pill is wider than tall inside it). */
export function shapeRadius(shape: Shape): string {
  return shape === "circle" ? "50%" : shape === "squircle" ? "32%" : "999px";
}
export function clamp(v: number, lo: number, hi: number): number { return Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : lo; }
