import type { Tab } from "../lib/model";
import type { SelfUpdate } from "../lib/native";
import { Gear, Mark, Refresh, Search } from "./Icons";
import { Kbd } from "./Kbd";

export type View = Tab | "settings";

const VIEWS: { id: Tab; label: string; keys: string }[] = [
  { id: "updates", label: "Updates", keys: "G then U" },
  { id: "current", label: "Up to date", keys: "G then C" },
  { id: "other", label: "Unsupported", keys: "G then S" },
  { id: "ignored", label: "Ignored", keys: "G then I" },
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
 * The window's title bar and navigation in one strip, like a Mac app's
 * unified toolbar: the window buttons sit on its left, the views in the
 * middle, actions on the right. Empty space drags the window.
 */
export function TopBar({ view, counts, checking, selfUpdate, onView, onCheck, onPalette }: Props) {
  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="topbar-brand" data-tauri-drag-region>
        <Mark />
        <span className="brand-name" data-tauri-drag-region>
          Camu
        </span>
      </div>

      <nav className="topbar-nav" aria-label="Views" data-tauri-drag-region>
        {VIEWS.map((v) => (
          <button
            key={v.id}
            type="button"
            className={`nav-item${view === v.id ? " active" : ""}`}
            onClick={() => onView(v.id)}
            title={`${v.label} (${v.keys})`}
            aria-current={view === v.id ? "page" : undefined}
          >
            {v.label}
            {counts[v.id] > 0 && (
              <span className={`nav-count${v.id === "updates" ? " hot" : ""}`}>{counts[v.id]}</span>
            )}
          </button>
        ))}
      </nav>

      <div className="topbar-actions" data-tauri-drag-region>
        <button type="button" className="search-pill" onClick={onPalette} title="Search or run a command">
          <Search />
          <span>Search</span>
          <Kbd keys={["⌘", "K"]} />
        </button>
        <button
          type="button"
          className="icon-btn"
          onClick={onCheck}
          disabled={checking}
          title={checking ? "Checking…" : "Check for updates (⌘R)"}
          aria-label="Check for updates"
        >
          <Refresh className={checking ? "i spin" : "i"} />
        </button>
        <button
          type="button"
          className={`icon-btn${view === "settings" ? " active" : ""}`}
          onClick={() => onView("settings")}
          title={selfUpdate ? `Settings — Camu ${selfUpdate.version} is available` : "Settings (⌘,)"}
          aria-label="Settings"
        >
          <Gear />
          {selfUpdate && <span className="badge-dot" aria-hidden />}
        </button>
      </div>
    </header>
  );
}
