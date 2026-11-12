import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Toggle } from "./Toggle";
import { Panel } from "./Panel";
import "./styles.css";

const which = new URLSearchParams(location.search).get("w");
document.documentElement.dataset.window = which ?? "panel";
createRoot(document.getElementById("root")!).render(<StrictMode>{which === "toggle" ? <Toggle /> : <Panel />}</StrictMode>);
