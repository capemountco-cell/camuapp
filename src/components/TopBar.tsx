import type { Tab } from "../lib/model";
import type { SelfUpdate } from "../lib/native";
import { ArrowUpCircle, CheckCircle, EyeOff, Gear, Question, Wordmark, Refresh, Search } from "./Icons";

export type View = Tab | "settings";

const PLACES: { id: Tab; label: string; keys: string; icon: React.ReactNode }[] = [
  { id: "updates", label: "Updates", keys: "G then U", icon: <ArrowUpCircle className="i tool-icon" /> },
  { id: "current", label: "Up to date", keys: "G then C", icon: <CheckCircle className="i tool-icon" /> },
  { id: "other", label: "Unsupported", keys: "G then S", icon: <Question className="i tool-icon" /> },
  { id: "ignored", label: "Ignored", keys: "G then I", icon: <EyeOff className="i tool-icon" /> },
];

interface Props {
  view: View;
  counts: Record<Tab, number>;
  checking: boolean;
  selfUpdate: SelfUpdate | null;
  onView: (view: View) => void;
  onCheck: () => void;
  onPalette: () => void;
}

/**
 * The window's top, laid out like a classic Mac toolbar (MacUpdater's):
 * the title row holds the window buttons and Camu's wordmark, centred; the row
 * below holds large icon-and-label buttons, the places on the left and the
 * actions on the right. Empty space in either row drags the window.
 */
export function TopBar({ view, counts, checking, selfUpdate, onView, onCheck, onPalette }: Props) {
  return (
    <header className="toolbar-shell" data-tauri-drag-region>
      <div className="titlerow" data-tauri-drag-region>
        <span className="title-logo" data-tauri-drag-region>
          <Wordmark height={13} />
        </span>
      </div>

      <nav className="toolrow" aria-label="Main" data-tauri-drag-region>
        <div className="toolgroup" data-tauri-drag-region>
          {PLACES.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`tool${view === p.id ? " current" : ""}`}
              aria-current={view === p.id ? "page" : undefined}
              title={`${p.label} (${p.keys})`}
              onClick={() => onView(p.id)}
            >
              {p.icon}
              <span className="tool-label">{p.label}</span>
              {p.id === "updates" && counts.updates > 0 && <span className="tool-badge">{counts.updates}</span>}
            </button>
          ))}
        </div>
        <div className="toolgroup" data-tauri-drag-region>
          <button
            type="button"
            className="tool narrow"
            onClick={onCheck}
            disabled={checking}
            title="Check for updates (⌘R)"
          >
            <Refresh className={checking ? "i tool-icon spin" : "i tool-icon"} />
            <span className="tool-label">{checking ? "Checking…" : "Check"}</span>
          </button>
          <button type="button" className="tool narrow" onClick={onPalette} title="Search or run a command (⌘K)">
            <Search className="i tool-icon" />
            <span className="tool-label">Search</span>
          </button>
          <button
            type="button"
            className={`tool narrow${view === "settings" ? " current" : ""}`}
            onClick={() => onView("settings")}
            title={selfUpdate ? `Settings — Camu ${selfUpdate.version} is available` : "Settings (⌘,)"}
          >
            <Gear className="i tool-icon" />
            <span className="tool-label">Settings</span>
            {selfUpdate && <span className="tool-dot" aria-label="Camu update available" />}
          </button>
        </div>
      </nav>
    </header>
  );
}
