/**
 * The few icons Camu needs, drawn at 16px on a 4px grid with 1.5px strokes.
 * Inline SVG: no icon font, no dependency.
 */

import type { Phase, Status } from "../lib/native";
import { phaseProgress } from "../lib/model";

type IconProps = { className?: string };

const svg = (children: React.ReactNode, { className }: IconProps = {}) => (
  <svg className={className ?? "i"} width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden>
    {children}
  </svg>
);

const stroke = { stroke: "currentColor", strokeWidth: 1.5, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };

export const ArrowUpCircle = (p: IconProps) =>
  svg(
    <>
      <circle cx="8" cy="8" r="6" {...stroke} />
      <path d="M8 10.5v-5M5.75 7.5 8 5.25l2.25 2.25" {...stroke} />
    </>,
    p,
  );
export const CheckCircle = (p: IconProps) =>
  svg(
    <>
      <circle cx="8" cy="8" r="6" {...stroke} />
      <path d="m5.5 8.25 1.75 1.75 3.25-3.5" {...stroke} />
    </>,
    p,
  );
export const Question = (p: IconProps) =>
  svg(
    <>
      <circle cx="8" cy="8" r="6" {...stroke} strokeDasharray="2 2.2" />
    </>,
    p,
  );
export const EyeOff = (p: IconProps) =>
  svg(
    <>
      <path d="M2.5 8s2-3.75 5.5-3.75S13.5 8 13.5 8s-2 3.75-5.5 3.75S2.5 8 2.5 8Z" {...stroke} />
      <path d="M3 13 13 3" {...stroke} />
    </>,
    p,
  );
export const Refresh = (p: IconProps) =>
  svg(<path d="M12.75 6.5A5 5 0 0 0 3.6 5.25M3.25 9.5a5 5 0 0 0 9.15 1.25M3.5 2.75v2.75h2.75M12.5 13.25V10.5H9.75" {...stroke} />, p);
export const Search = (p: IconProps) =>
  svg(
    <>
      <circle cx="7" cy="7" r="4.25" {...stroke} />
      <path d="m10.25 10.25 3 3" {...stroke} />
    </>,
    p,
  );
export const Close = (p: IconProps) => svg(<path d="m4.5 4.5 7 7M11.5 4.5l-7 7" {...stroke} />, p);
export const External = (p: IconProps) => svg(<path d="M6.5 3.5h-3v9h9v-3M9 3.5h3.5V7M12.5 3.5 7.5 8.5" {...stroke} />, p);
export const Folder = (p: IconProps) => svg(<path d="M2.5 4.5v7.25c0 .41.34.75.75.75h9.5c.41 0 .75-.34.75-.75V6.25a.75.75 0 0 0-.75-.75H8L6.5 3.75H3.25a.75.75 0 0 0-.75.75Z" {...stroke} />, p);
export const Undo = (p: IconProps) => svg(<path d="M5.5 3.5 2.75 6.25 5.5 9M3 6.25h6.25a3.5 3.5 0 0 1 0 7H7" {...stroke} />, p);
export const Command = (p: IconProps) =>
  svg(<path d="M6 6V4.5a1.5 1.5 0 1 0-1.5 1.5H6Zm0 0v4m0-4h4m-4 4H4.5A1.5 1.5 0 1 0 6 11.5V10Zm0 0h4m0-4V4.5A1.5 1.5 0 1 1 11.5 6H10Zm0 0v4m0 0v1.5a1.5 1.5 0 1 0 1.5-1.5H10Z" {...stroke} />, p);

/** Camu's mark: an eye whose pupil is an up arrow. */
export const Mark = () => (
  <svg className="mark" width="18" height="18" viewBox="0 0 18 18" aria-hidden>
    <rect width="18" height="18" rx="5" fill="var(--accent)" />
    <path d="M9 12.5V6M6.5 8.25 9 5.75l2.5 2.5" stroke="#fff" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);

/**
 * The status glyph at the start of each row, like an issue's status circle:
 * it shows the check result, and turns into a progress ring while updating.
 */
export function StatusGlyph({ status, phase }: { status: Status | undefined; phase: Phase | undefined }) {
  if (phase) {
    if (phase.phase === "done") return <CheckCircle className="i s-done" />;
    if (phase.phase === "failed")
      return svg(
        <>
          <circle cx="8" cy="8" r="6" {...stroke} />
          <path d="m6 6 4 4m0-4-4 4" {...stroke} />
        </>,
        { className: "i s-failed" },
      );
    if (phase.phase === "handoff") return <External className="i s-handoff" />;
    const progress = phaseProgress(phase);
    const r = 5.5;
    const c = 2 * Math.PI * r;
    return (
      <svg className="i s-busy" width="16" height="16" viewBox="0 0 16 16" aria-hidden>
        <circle cx="8" cy="8" r={r} stroke="var(--line-strong)" strokeWidth="2" fill="none" />
        <circle
          className={progress === null ? "spin" : undefined}
          cx="8"
          cy="8"
          r={r}
          stroke="currentColor"
          strokeWidth="2"
          fill="none"
          strokeLinecap="round"
          strokeDasharray={`${progress === null ? c * 0.3 : Math.max(c * progress, 1)} ${c}`}
          transform="rotate(-90 8 8)"
        />
      </svg>
    );
  }
  switch (status) {
    case "updateAvailable":
      return <ArrowUpCircle className="i s-update" />;
    case "upToDate":
      return <CheckCircle className="i s-current" />;
    case "failed":
    case "unsupported":
      return <Question className="i s-other" />;
    default:
      return <span className="i skeleton-dot" />;
  }
}
