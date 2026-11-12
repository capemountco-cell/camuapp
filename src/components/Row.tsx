import { forwardRef } from "react";

import { otherReason, phaseLabel, sourceLabel, type Row as RowData } from "../lib/model";
import type { Phase } from "../lib/native";
import { AppIcon } from "./AppIcon";
import { StatusGlyph } from "./Icons";

interface Props {
  row: RowData;
  phase: Phase | undefined;
  selected: boolean;
  open: boolean;
  onSelect: () => void;
  onOpen: () => void;
}

/** One app, one 36px line. Details live in the side panel, not here. */
export const Row = forwardRef<HTMLLIElement, Props>(function Row({ row, phase, selected, open, onSelect, onOpen }, ref) {
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
        <span className="to">{best.version}</span>
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
      className={`row${selected ? " selected" : ""}${open ? " open" : ""}`}
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
      <span className="row-fill" />
      {right}
    </li>
  );
});
