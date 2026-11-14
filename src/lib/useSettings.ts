import { useEffect, useState } from "react";
import { api } from "./api";
import type { Settings } from "./types";
import { resolveTheme } from "./theme";

/** Loads the settings, follows changes made from ANY window (one source of truth) and applies the look: theme, accent, text scale, density, surface opacity, motion. */
export function useSettings(): Settings | null {
  const [s, setS] = useState<Settings | null>(null);
  useEffect(() => {
    let off: (() => void) | undefined;
    api.getSettings().then(setS);
    api.onSettings(setS).then((f) => { off = f; });
    return () => off?.();
  }, []);
  useEffect(() => {
    if (!s) return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)"); const root = document.documentElement;
    const apply = () => { root.dataset.theme = resolveTheme(s.theme, mq.matches); root.dataset.motion = s.toggle.animations ? "on" : "off"; };
    apply(); mq.addEventListener("change", apply);
    if (root.dataset.window !== "toggle") {
      root.style.setProperty("--accent", s.ui.accent ?? s.toggle.accent); root.style.setProperty("--scale", String(s.ui.text_size));
      root.style.setProperty("--surface-a", String(s.ui.surface_opacity)); root.dataset.density = s.ui.density;
    }
    return () => mq.removeEventListener("change", apply);
  }, [s]);
  return s;
}
