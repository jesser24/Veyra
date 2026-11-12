import { useEffect, useState } from "react";
import { api } from "./api";
import type { Settings } from "./types";
import { resolveTheme } from "./theme";

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
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => { document.documentElement.dataset.theme = resolveTheme(s.theme, mq.matches); document.documentElement.dataset.motion = s.toggle.animations ? "on" : "off"; };
    apply(); mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [s]);
  return s;
}
