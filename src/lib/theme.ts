import type { Settings, Shape, Side } from "./types";

export function resolveTheme(theme: Settings["theme"], systemDark: boolean): "light" | "dark" {
  return theme === "system" ? (systemDark ? "dark" : "light") : theme;
}
/** CSS border-radius of the drawn tab. A tab is rounded on the side facing the screen interior and flat against the edge. */
export function tabRadius(shape: Shape, side: Side | null, thickness: number): string {
  const r = Math.round(thickness * 1.2);
  if (shape === "pill" || side === null) return "999px";
  if (shape === "rounded") return `${Math.round(thickness * 0.6)}px`;
  switch (side) {
    case "left": return `0 ${r}px ${r}px 0`;
    case "right": return `${r}px 0 0 ${r}px`;
    case "top": return `0 0 ${r}px ${r}px`;
    case "bottom": return `${r}px ${r}px 0 0`;
  }
}
export function clamp(v: number, lo: number, hi: number): number { return Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : lo; }
