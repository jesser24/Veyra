import { useEffect, useState } from "react";
import { useSettings } from "../lib/useSettings";
import { useEngine, useListen } from "../lib/hooks";
import { api } from "../lib/api";
import type { Settings } from "../lib/types";
import { Icon, type IconName } from "../components/ui";
import { AiSection } from "./AiSection"; import { ModelsSection } from "./ModelsSection"; import { ConversationsSection } from "./ConversationsSection"; import { AppearanceSection } from "./AppearanceSection";
import { ToggleSection } from "./ToggleSection"; import { DocumentsSection } from "./DocumentsSection"; import { NetworkSection } from "./NetworkSection"; import { SecuritySection } from "./SecuritySection";
import { BackupsSection } from "./BackupsSection"; import { AdvancedSection } from "./AdvancedSection"; import { Setup } from "./Setup";

const SECTIONS: Array<{ id: string; label: string; icon: IconName; blurb: string }> = [
  { id: "ai", label: "Artificial intelligence", icon: "brain", blurb: "How the local model behaves." },
  { id: "models", label: "Models", icon: "layers", blurb: "Download, verify and load models." },
  { id: "conversations", label: "Conversations", icon: "chat", blurb: "Everything you have discussed." },
  { id: "appearance", label: "Appearance", icon: "palette", blurb: "Theme, colour, size and comfort." },
  { id: "toggle", label: "Toggle", icon: "toggle", blurb: "The tab on the screen edge." },
  { id: "documents", label: "Documents", icon: "doc", blurb: "Files you can attach, and how they are read." },
  { id: "network", label: "Network & Privacy", icon: "globe", blurb: "Every connection Veyra makes." },
  { id: "security", label: "Security & storage", icon: "lock", blurb: "How your data is protected." },
  { id: "backups", label: "Backups", icon: "save", blurb: "Copy and restore your data." },
  { id: "advanced", label: "Advanced", icon: "sliders", blurb: "Engine tuning and measurements." },
];

/** The full application. Same backend, same events and same design system as the chat window: a change here is visible there immediately. */
export function AppWindow() {
  const s = useSettings(); const eng = useEngine();
  const [sec, setSec] = useState(() => new URLSearchParams(location.search).get("section") ?? "ai");
  useListen(() => api.onAppSection((x) => setSec(x)), []);
  const [draft, setDraft] = useState<Settings | null>(null); useEffect(() => { if (s) setDraft(s); }, [s]);
  if (!s || !draft) return null;
  // optimistic: the control moves at once, the backend validates/clamps and broadcasts the final value to every window
  const push = (n: Settings) => { setDraft(n); void api.updateSettings(n).then(setDraft).catch(() => setDraft(s)); };
  const cur = SECTIONS.find((x) => x.id === sec) ?? SECTIONS[0];
  const body = () => { switch (cur.id) {
    case "ai": return <AiSection s={draft} push={push} />; case "models": return <ModelsSection />; case "conversations": return <ConversationsSection />; case "appearance": return <AppearanceSection s={draft} push={push} />;
    case "toggle": return <ToggleSection s={draft} push={push} />; case "documents": return <DocumentsSection />; case "network": return <NetworkSection />; case "security": return <SecuritySection />;
    case "backups": return <BackupsSection />; default: return <AdvancedSection s={draft} push={push} />; } };
  if (!s.setup_done) return <div className="appwin setupwin"><Setup /></div>;
  return (
    <div className="appwin">
      <nav className="side" aria-label="Sections">
        <div className="side-brand"><img src="/veyra-mark.png" alt="" draggable={false} /><b>Veyra</b></div>
        <ul>{SECTIONS.map((x) => <li key={x.id}><button type="button" className={x.id === cur.id ? "on" : ""} aria-current={x.id === cur.id ? "page" : undefined} onClick={() => setSec(x.id)}><Icon name={x.icon} size={17} /><span>{x.label}</span></button></li>)}</ul>
        <div className="side-foot"><span className={`pulse ${eng?.status.loaded ? "ok" : eng?.loading ? "busy" : "off"}`} aria-hidden="true" /><small>{eng?.status.loaded ? "Model ready" : eng?.loading ? "Loading model…" : "No model loaded"}</small></div>
      </nav>
      <main className="content"><header className="page-head"><h1>{cur.label}</h1><p>{cur.blurb}</p></header><div className="page" key={cur.id}>{body()}</div></main>
    </div>
  );
}
