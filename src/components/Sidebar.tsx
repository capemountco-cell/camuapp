import type { Tab } from "../lib/model";
import { ArrowUpCircle, CheckCircle, EyeOff, Mark, Question, Refresh, Search } from "./Icons";
import { Kbd } from "./Kbd";

const VIEWS: { id: Tab; label: string; icon: React.ReactNode; keys: string[] }[] = [
  { id: "updates", label: "Updates", icon: <ArrowUpCircle />, keys: ["G", "U"] },
  { id: "current", label: "Up to date", icon: <CheckCircle />, keys: ["G", "C"] },
  { id: "other", label: "Unsupported", icon: <Question />, keys: ["G", "S"] },
  { id: "ignored", label: "Ignored", icon: <EyeOff />, keys: ["G", "I"] },
];

interface Props {
  tab: Tab;
  counts: Record<Tab, number>;
  checking: boolean;
  status: string;
  onTab: (tab: Tab) => void;
  onCheck: () => void;
  onPalette: () => void;
}

export function Sidebar({ tab, counts, checking, status, onTab, onCheck, onPalette }: Props) {
  return (
    <nav className="sidebar" aria-label="Views">
      <div className="brand">
        <Mark />
        <span className="brand-name">Camu</span>
      </div>

      <button type="button" className="side-search" onClick={onPalette}>
        <Search />
        <span>Search or run…</span>
        <Kbd keys={["⌘", "K"]} />
      </button>

      <div className="side-group">
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            className={`side-item${tab === v.id ? " active" : ""}`}
            onClick={() => onTab(v.id)}
            title={`${v.label} (${v.keys.join(" then ")})`}
          >
            {v.icon}
            <span className="side-label">{v.label}</span>
            {counts[v.id] > 0 && <span className="side-count">{counts[v.id]}</span>}
          </button>
        ))}
      </div>

      <div className="side-foot">
        <button type="button" className="side-item" onClick={onCheck} disabled={checking}>
          <Refresh className={checking ? "i spin" : "i"} />
          <span className="side-label">{checking ? "Checking…" : "Check for updates"}</span>
          <Kbd keys={["⌘", "R"]} />
        </button>
        <p className="side-status">{status}</p>
      </div>
    </nav>
  );
}
