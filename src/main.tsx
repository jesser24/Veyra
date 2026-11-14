import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Toggle } from "./Toggle";
import { ChatWindow } from "./chat/ChatWindow";
import { AppWindow } from "./app/AppWindow";
import "./styles/tokens.css"; import "./styles/components.css"; import "./styles/chat.css"; import "./styles/app.css"; import "./styles/toggle.css";

const which = new URLSearchParams(location.search).get("w") ?? "panel";
document.documentElement.dataset.window = which;
createRoot(document.getElementById("root")!).render(<StrictMode>{which === "toggle" ? <Toggle /> : which === "app" ? <AppWindow /> : <ChatWindow />}</StrictMode>);
