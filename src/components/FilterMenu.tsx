import { useEffect, useState } from "react";

import { sourceLabel, type SourceFilter } from "../lib/model";
import type { Source } from "../lib/native";

const SOURCES: Source[] = ["homebrew", "sparkle", "appStore", "electron", "github"];

interface Props {
  value: SourceFilter;
  counts: Record<SourceFilter, number>;
  onChange: (value: SourceFilter) => void;
}

/** "Filter" as an option menu, not a search box: narrow the list to one update source. */
export function FilterMenu({ value, counts, onChange }: Props) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [open]);

  const pick = (v: SourceFilter) => {
    onChange(v);
    setOpen(false);
  };
  const active = value !== "all";

  return (
    <div className="filter-menu">
      <button
        type="button"
        className={`filter-button${active ? " active" : ""}`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden>
          <path d="M2.5 4h11M4.5 8h7M6.5 12h3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
        {active ? <span className={`src src-${value}`}>{sourceLabel[value as Source]}</span> : "Filter"}
        <svg width="10" height="10" viewBox="0 0 12 12" aria-hidden className="chev">
          <path d="M3 4.5 6 7.5l3-3" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <>
          <div className="menu-scrim" onClick={() => setOpen(false)} role="presentation" />
          <div className="menu" role="menu" aria-label="Filter by source">
            <div className="menu-label">Source</div>
            <button type="button" role="menuitemradio" aria-checked={value === "all"} className="menu-row" onClick={() => pick("all")}>
              <span>All sources</span>
              <span className="menu-count">{counts.all}</span>
              <Check on={value === "all"} />
            </button>
            {SOURCES.map((s) => (
              <button
                key={s}
                type="button"
                role="menuitemradio"
                aria-checked={value === s}
                className="menu-row"
                disabled={counts[s] === 0 && value !== s}
                onClick={() => pick(s)}
              >
                <span className={`src src-${s}`}>{sourceLabel[s]}</span>
                <span className="menu-count">{counts[s]}</span>
                <Check on={value === s} />
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function Check({ on }: { on: boolean }) {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden className="menu-check" style={{ opacity: on ? 1 : 0 }}>
      <path d="m3.5 8.5 3 3 6-7" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
