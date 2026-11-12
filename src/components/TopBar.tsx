import { useEffect, useState } from "react";

import type { Tab } from "../lib/model";
import type { SelfUpdate } from "../lib/native";
import { ArrowUpCircle, CheckCircle, Gear, Mark, Refresh, Search } from "./Icons";
import { Kbd } from "./Kbd";

export type View = Tab | "settings";

const PLACES: { id: Tab; label: string; keys: string }[] = [
  { id: "updates", label: "Updates", keys: "G then U" },
  { id: "current", label: "Up to date", keys: "G then C" },
  { id: "other", label: "Unsupported", keys: "G then S" },
  { id: "ignored", label: "Ignored", keys: "G then I" },
];

/** What the status switcher says, and its colour. */
export interface Status {
  tone: "none" | "update" | "working" | "done";
  title: string;
  line: string;
}

interface Props {
  view: View;
  counts: Record<Tab, number>;
  status: Status;
  checking: boolean;
  canUpdateAll: number;
  selfUpdate: SelfUpdate | null;
  onView: (view: View) => void;
  onCheck: () => void;
  onUpdateAll: () => void;
  onPalette: () => void;
}

/**
 * The global bar, built like Banker's: the Camu mark, the four places in a
 * segmented control, and a status switcher that says what Camu is doing and
 * opens the actions. It is also the window's title bar: the window buttons
 * sit at its left end, and empty space drags the window.
 */
export function TopBar(props: Props) {
  const { view, counts, status, onView } = props;
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [open]);

  const run = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <>
      <header className="globalnav" data-tauri-drag-region>
        <button type="button" className="brand" onClick={() => onView("updates")}>
          <Mark />
          Camu
        </button>

        <nav aria-label="Main" className="places" data-tauri-drag-region>
          {PLACES.map((p) => (
            <button
              key={p.id}
              type="button"
              className={view === p.id ? "current" : ""}
              aria-current={view === p.id ? "page" : undefined}
              title={`${p.label} (${p.keys})`}
              onClick={() => onView(p.id)}
            >
              {p.label}
              {counts[p.id] > 0 && (
                <span className={`place-count${p.id === "updates" ? " hot" : ""}`}>{counts[p.id]}</span>
              )}
            </button>
          ))}
        </nav>

        <button
          type="button"
          className={`switcher tone-${status.tone}`}
          aria-expanded={open}
          aria-haspopup="menu"
          onClick={() => setOpen(!open)}
        >
          <span className="switcher-tile" aria-hidden>
            {status.tone === "working" ? (
              <Refresh className="i spin" />
            ) : status.tone === "update" ? (
              <ArrowUpCircle />
            ) : status.tone === "done" ? (
              <CheckCircle />
            ) : (
              <span className="switcher-dot" />
            )}
          </span>
          <span className="switcher-text">
            <span className="switcher-name">{status.title}</span>
            <span className="switcher-line">{status.line}</span>
          </span>
          {props.selfUpdate && <span className="badge-dot" aria-label="Camu update available" />}
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden className="chevron">
            <path
              d={open ? "M3 7.5 6 4.5l3 3" : "M3 4.5 6 7.5l3-3"}
              stroke="currentColor"
              strokeWidth="1.6"
              fill="none"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </header>

      {open && (
        <>
          <div className="menu-scrim" onClick={() => setOpen(false)} role="presentation" />
          <section className="status-menu" role="menu" aria-label="Actions">
            <button
              type="button"
              role="menuitem"
              className="status-item"
              disabled={props.checking}
              onClick={run(props.onCheck)}
            >
              <Refresh />
              <span>{props.checking ? "Checking…" : "Check for updates"}</span>
              <Kbd keys={["⌘", "R"]} />
            </button>
            <button
              type="button"
              role="menuitem"
              className="status-item"
              disabled={props.canUpdateAll === 0}
              onClick={run(props.onUpdateAll)}
            >
              <ArrowUpCircle />
              <span>{props.canUpdateAll > 0 ? `Update all ${props.canUpdateAll}` : "Update all"}</span>
              <Kbd keys={["⌘", "U"]} />
            </button>
            <button type="button" role="menuitem" className="status-item" onClick={run(props.onPalette)}>
              <Search />
              <span>Search or run a command</span>
              <Kbd keys={["⌘", "K"]} />
            </button>
            <div className="menu-rule" />
            <button type="button" role="menuitem" className="status-item" onClick={run(() => onView("settings"))}>
              <Gear />
              <span>Settings</span>
              <Kbd keys={["⌘", ","]} />
            </button>
            {props.selfUpdate && (
              <button
                type="button"
                role="menuitem"
                className="status-item notice"
                onClick={run(() => onView("settings"))}
              >
                <span className="dot" aria-hidden />
                <span>Camu {props.selfUpdate.version} is available</span>
              </button>
            )}
          </section>
        </>
      )}
    </>
  );
}
