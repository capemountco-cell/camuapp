import { forwardRef } from "react";

import { holdLabel, intelOnly, needsYou, noticeLabel, otherReason, phaseLabel, sourceLabel, type Row as RowData } from "../lib/model";
import type { Arch, Phase } from "../lib/native";
import { AppIcon } from "./AppIcon";
import { StatusGlyph } from "./Icons";

interface Props {
  row: RowData;
  phase: Phase | undefined;
  selected: boolean;
  open: boolean;
  onSelect: () => void;
  onOpen: () => void;
  /** This Mac's chip, for the "Intel only" badge. */
  host?: Arch | null;
  askMajor?: boolean;
}

/** The one badge a row shows, most important first. */
function badge(row: RowData, host: Arch | null, askMajor: boolean): { text: string; tone: string } | null {
  const r = row.report;
  if (r?.hold) return { text: holdLabel(r.hold), tone: "" };
  if (r?.notice && r.notice.kind !== "setapp") return { text: noticeLabel(r.notice), tone: r.notice.kind === "vendor" ? "info" : "" };
  if (r && needsYou(r, askMajor) && r.major) return { text: "Major version · may need a new licence", tone: "warn" };
  if (intelOnly(row.app, host)) return { text: "Intel only · runs through Rosetta", tone: "" };
  return null;
}

/** One app, one 36px line. Details live in the side panel, not here. */
export const Row = forwardRef<HTMLLIElement, Props>(function Row({ row, phase, selected, open, onSelect, onOpen, host = null, askMajor = true }, ref) {
  const tag = badge(row, host, askMajor);
  const { app, report } = row;
  const best = report?.best;
  const hasUpdate = report?.status === "updateAvailable";

  let right: React.ReactNode;
  if (phase) {
    right = <span className={`phase p-${phase.phase}`}>{phaseLabel(phase)}</span>;
  } else if (hasUpdate && best) {
    right = (
      <span className="versions">
        <span className="from">{app.version}</span>
        <span className="arrow">→</span>
        <span className="vpill">
          <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden>
            <rect x="1" y="1" width="12" height="12" rx="3.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
            <path d="M7 4.5 9.6 9H4.4Z" fill="currentColor" />
          </svg>
          {best.version}
        </span>
      </span>
    );
  } else if (!report || report.status === "upToDate") {
    right = <span className="from">{app.version}</span>;
  } else {
    right = <span className="faint">{otherReason(row)}</span>;
  }

  return (
    <li
      ref={ref}
      className={`row${selected ? " selected" : ""}${open ? " open" : ""}${report?.hold ? " held-row" : ""}`}
      onMouseDown={onSelect}
      onDoubleClick={onOpen}
      onClick={onOpen}
      aria-selected={selected}
      role="option"
    >
      <StatusGlyph status={report?.status} phase={phase} />
      <AppIcon app={app} />
      <span className="row-name">{app.name}</span>
      {best && <span className={`row-source src src-${best.source}`}>{sourceLabel[best.source]}</span>}
      {tag && <span className={`badge${tag.tone ? ` ${tag.tone}` : ""}`}>{tag.text}</span>}
      <span className="row-fill" />
      {right}
    </li>
  );
});
