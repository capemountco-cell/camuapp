import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useRef, useState, type CSSProperties, type MouseEvent, type ReactNode, type RefObject } from "react";
import wordmark from "@/assets/camu-wordmark-white.svg.asset.json";
import updates from "@/assets/camu-updates.png.asset.json";
import detail from "@/assets/camu-detail.png.asset.json";
import menubar from "@/assets/camu-menubar.png.asset.json";

const DMG = "/download";
const TITLE = "Camu: keep every Mac app up to date";
const DESC =
  "Camu checks every app on your Mac against every source that knows it and updates them all in one click. Free for Mac. No account, no tracking, no subscription.";
const OG_IMAGE = "https://camu.cc/og-image.png";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: TITLE },
      { name: "description", content: DESC },
      { property: "og:title", content: TITLE },
      { property: "og:description", content: DESC },
      { property: "og:type", content: "website" },
      { property: "og:site_name", content: "Camu" },
      { property: "og:url", content: "https://camu.cc/" },
      { property: "og:image", content: OG_IMAGE },
      { property: "og:image:width", content: "1200" },
      { property: "og:image:height", content: "630" },
      { property: "og:image:alt", content: "Camu: Every app on your Mac, up to date." },
      { name: "twitter:card", content: "summary_large_image" },
      { name: "twitter:title", content: TITLE },
      { name: "twitter:description", content: DESC },
      { name: "twitter:image", content: OG_IMAGE },
    ],
    links: [{ rel: "canonical", href: "https://camu.cc/" }],
  }),
  component: Index,
});

const SOURCES = ["Sparkle", "Mac App Store", "Homebrew", "Electron", "GitHub Releases"];

// The six smaller features. `place` positions each in the desktop bento grid
// (literal class strings so Tailwind sees them).
const minor = [
  { n: "02", t: "Update All", d: "One click updates everything, Homebrew apps included. If an app needs your admin password, you're asked once for the whole batch.", place: "lg:col-start-3 lg:row-start-1" },
  { n: "04", t: "Release notes inline", d: "Read what changed before you update.", place: "lg:col-start-4 lg:row-start-1" },
  { n: "05", t: "Menu bar", d: "The update count lives in your menu bar, with Update All one click away.", place: "lg:col-start-3 lg:row-start-2" },
  { n: "06", t: "Keeps itself current", d: "Camu checks daily and installs its own updates automatically.", place: "lg:col-start-4 lg:row-start-2" },
  { n: "07", t: "Ignore and betas", d: "Skip an app, or opt one into beta versions.", place: "lg:col-start-1 lg:col-span-2 lg:row-start-3" },
  { n: "08", t: "No strings", d: "No account, no analytics, no ads, no subscription.", place: "lg:col-start-1 lg:col-span-2 lg:row-start-4" },
];

const META = ["Free beta", "macOS 12 or later", "Apple silicon and Intel"];

/* ---------- small pieces ---------- */

function AppleLogo({ className = "" }: { className?: string }) {
  return (
    <svg viewBox="0 0 814 1000" aria-hidden="true" className={className} fill="currentColor">
      <path d="M788 341c-6 4-108 62-108 190 0 149 131 202 135 203-1 3-21 72-69 142-43 62-88 124-156 124s-86-40-165-40c-77 0-104 41-166 41s-106-57-156-127C45 792 0 667 0 548 0 357 124 256 247 256c65 0 120 43 161 43 39 0 100-45 174-45 28 0 129 2 206 87zM554 163c31-37 53-88 53-139 0-7-1-14-2-20-50 2-110 34-146 76-28 32-55 83-55 135 0 8 1 16 2 18 3 1 9 2 14 2 45 0 102-30 134-72z" />
    </svg>
  );
}

function Eyebrow({ n, children }: { n: string; children: ReactNode }) {
  return (
    <p className="label-mono flex items-center gap-3">
      <span className="text-download">{n}</span>
      <span className="h-px w-6 bg-border-strong" />
      <span>{children}</span>
    </p>
  );
}

function MetaLine() {
  return (
    <p className="label-mono mt-6 flex flex-wrap items-center justify-center gap-x-3 gap-y-1">
      {META.map((m, i) => (
        // On phones the last item drops to its own line, without a stray leading dot.
        <span key={m} className={`flex items-center justify-center gap-3 ${i === META.length - 1 ? "basis-full sm:basis-auto" : ""}`}>
          {i > 0 && (
            <span className={`size-[3px] rounded-full bg-muted-foreground/60 ${i === META.length - 1 ? "hidden sm:block" : ""}`} />
          )}
          {m}
        </span>
      ))}
    </p>
  );
}

function DownloadButton({ className = "" }: { className?: string }) {
  return (
    <a href={DMG} className={`btn-download h-12 px-6 text-[15px] ${className}`}>
      <AppleLogo className="-mt-0.5 size-4" />
      Download for Mac
    </a>
  );
}

function spotlight(e: MouseEvent<HTMLElement>) {
  const r = e.currentTarget.getBoundingClientRect();
  e.currentTarget.style.setProperty("--mx", `${e.clientX - r.left}px`);
  e.currentTarget.style.setProperty("--my", `${e.clientY - r.top}px`);
}

/** Five sources converging on one answer. */
function SourcesVisual() {
  const ys = [10, 30, 50, 70, 90];
  return (
    <div className="relative mt-8 grid h-48 grid-cols-[auto_1fr_auto] items-stretch gap-0 sm:h-56">
      <div className="grid grid-rows-5">
        {SOURCES.map((s) => (
          <div key={s} className="flex items-center">
            <span className="rounded-md border border-border-strong bg-surface px-2 py-1 font-mono text-[10.5px] text-secondary-text sm:text-[11px]">
              {s}
            </span>
          </div>
        ))}
      </div>
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="h-full w-full" aria-hidden="true">
        {ys.map((y) => (
          <g key={y}>
            <path d={`M0 ${y} C 55 ${y}, 45 50, 100 50`} fill="none" stroke="oklch(1 0 0 / 0.12)" strokeWidth="1" vectorEffect="non-scaling-stroke" />
            <path d={`M0 ${y} C 55 ${y}, 45 50, 100 50`} fill="none" stroke="oklch(0.8253 0.1574 83.6 / 0.7)" strokeWidth="1" vectorEffect="non-scaling-stroke" className="flow-line" />
          </g>
        ))}
      </svg>
      <div className="flex items-center">
        <span className="rounded-md border border-download/40 bg-download/10 px-2.5 py-1.5 font-mono text-xs font-medium text-download shadow-[0_0_24px_-4px_oklch(0.8253_0.1574_83.6/0.5)]">
          3.7.7
        </span>
      </div>
    </div>
  );
}

/** What happens before an update replaces anything. */
function SafetyVisual() {
  const rows: [string, string, string, string][] = [
    ["✓", "signature", "matches developer", "text-success"],
    ["✓", "backup", "created", "text-success"],
    ["✓", "install", "3.6.11 → 3.7.7", "text-success"],
    ["↺", "roll back", "available", "text-download"],
  ];
  return (
    <div className="mt-8 overflow-hidden rounded-xl border border-border bg-background/60 font-mono text-[11.5px] sm:text-xs">
      <div className="flex items-center gap-1.5 border-b border-border px-3 py-2">
        <span className="size-2 rounded-full bg-white/10" />
        <span className="size-2 rounded-full bg-white/10" />
        <span className="size-2 rounded-full bg-white/10" />
        <span className="ml-2 text-muted-foreground">iTerm.app</span>
      </div>
      <ul className="space-y-2 px-3 py-3">
        {rows.map(([icon, k, v, c]) => (
          <li key={k} className="flex gap-3">
            <span className={c}>{icon}</span>
            <span className="w-20 shrink-0 text-muted-foreground">{k}</span>
            <span className="min-w-0 truncate text-secondary-text">{v}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/* ---------- page ---------- */

function useScrolled() {
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const on = () => setScrolled(window.scrollY > 8);
    on();
    window.addEventListener("scroll", on, { passive: true });
    return () => window.removeEventListener("scroll", on);
  }, []);
  return scrolled;
}

/** Fade sections up as they enter, without hiding anything before JS runs. */
function useReveal(root: RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const el = root.current;
    if (!el || !("IntersectionObserver" in window)) return;
    const items = Array.from(el.querySelectorAll<HTMLElement>("[data-reveal]"));
    // Anything already on screen stays visible: no flash on load.
    for (const it of items) {
      if (it.getBoundingClientRect().top < window.innerHeight) it.setAttribute("data-shown", "");
    }
    el.classList.add("reveal-ready");
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            e.target.setAttribute("data-shown", "");
            io.unobserve(e.target);
          }
        }
      },
      { rootMargin: "0px 0px -8% 0px" },
    );
    items.forEach((it) => !it.hasAttribute("data-shown") && io.observe(it));
    return () => io.disconnect();
  }, [root]);
}

/** Hero screenshot leans back and settles flat as it scrolls up. */
function useTilt(target: RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const el = target.current;
    if (!el) return;
    let raf = 0;
    const update = () => {
      raf = 0;
      const r = el.getBoundingClientRect();
      const vh = window.innerHeight;
      const p = Math.min(1, Math.max(0, (vh - r.top) / (vh * 0.75)));
      el.style.setProperty("--tilt", (14 * (1 - p)).toFixed(2));
    };
    const on = () => { if (!raf) raf = requestAnimationFrame(update); };
    update();
    window.addEventListener("scroll", on, { passive: true });
    window.addEventListener("resize", on);
    return () => {
      window.removeEventListener("scroll", on);
      window.removeEventListener("resize", on);
      cancelAnimationFrame(raf);
    };
  }, [target]);
}

const navLink = "text-sm text-secondary-text transition-colors hover:text-foreground";

function Index() {
  const root = useRef<HTMLDivElement>(null);
  const hero = useRef<HTMLDivElement>(null);
  const scrolled = useScrolled();
  useReveal(root);
  useTilt(hero);

  return (
    <div ref={root} className="min-h-screen bg-background text-foreground">
      {/* Nav */}
      <header
        className={`fixed inset-x-0 top-0 z-50 border-b transition-[background-color,border-color,backdrop-filter] duration-300 ${
          scrolled ? "border-border bg-background/70 backdrop-blur-xl backdrop-saturate-150" : "border-transparent"
        }`}
      >
        <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-5">
          <a href="#top" aria-label="Camu home" className="-m-2 p-2">
            <img src={wordmark.url} alt="Camu" width={381} height={62} className="h-auto w-[76px]" />
          </a>
          <nav className="flex items-center gap-6">
            <a href="#features" className={`${navLink} hidden sm:inline`}>Features</a>
            <a href="#install" className={`${navLink} hidden sm:inline`}>Install</a>
            <a href={DMG} className="btn-download h-9 px-3.5 text-[13px]">
              <AppleLogo className="-mt-px size-3" />
              Download
            </a>
          </nav>
        </div>
      </header>

      <main id="top" className="overflow-x-clip">
        {/* Hero */}
        <section className="relative overflow-hidden">
          <div className="grid-backdrop" />
          <div className="relative mx-auto max-w-6xl px-5 pt-32 pb-20 text-center sm:pt-40 sm:pb-28">
            <div data-reveal className="inline-flex items-center gap-2.5 rounded-full border border-border-strong bg-white/[0.03] py-1.5 pr-3.5 pl-3 backdrop-blur">
              <span className="pulse-dot" />
              <span className="label-mono text-secondary-text">Free beta for macOS</span>
            </div>
            <h1
              data-reveal
              style={{ "--d": "60ms" } as CSSProperties}
              className="text-sheen mx-auto mt-7 max-w-4xl pb-2 text-[clamp(2.75rem,7.5vw,5.75rem)] leading-[1.02] font-semibold tracking-[-0.045em] text-balance"
            >
              Every app on your Mac, up to date.
            </h1>
            <p
              data-reveal
              style={{ "--d": "120ms" } as CSSProperties}
              className="mx-auto mt-6 max-w-[34rem] text-base leading-relaxed text-secondary-text text-pretty sm:text-lg"
            >
              Camu checks each app against every source that knows it and updates them all in one click. No account, no tracking, no subscription.
            </p>
            <div data-reveal style={{ "--d": "180ms" } as CSSProperties}>
              <div className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row">
                <DownloadButton className="w-full max-w-xs sm:w-auto" />
                <a href="#install" className="btn-ghost h-12 w-full max-w-xs px-5 text-[15px] sm:w-auto">
                  How to install <span aria-hidden="true">→</span>
                </a>
              </div>
              <MetaLine />
            </div>

            <div ref={hero} data-reveal style={{ "--d": "240ms" } as CSSProperties} className="tilt-stage relative mt-16 sm:mt-20">
              <div className="glow -inset-x-10 -top-10 bottom-0 sm:-inset-x-24" />
              <div className="frame tilt">
                <img src={updates.url} width={2560} height={1600} className="block h-auto w-full"
                  alt="Camu main window listing installed Mac apps with available updates and an Update all button" />
              </div>
            </div>
          </div>
        </section>

        {/* Sources strip */}
        <section aria-label="Update sources" className="border-y border-border bg-surface/40">
          <div className="mx-auto flex max-w-6xl flex-col items-center gap-4 px-5 py-6 lg:flex-row lg:justify-between">
            <p className="label-mono shrink-0">Checks against</p>
            <ul className="flex flex-wrap items-center justify-center gap-x-0 gap-y-2">
              {SOURCES.map((s, i) => (
                <li key={s} className={`px-4 font-mono text-[13px] text-secondary-text sm:px-6 ${i > 0 ? "border-l border-border-strong" : ""}`}>
                  {s}
                </li>
              ))}
            </ul>
          </div>
        </section>

        {/* Features */}
        <section id="features" className="mx-auto max-w-6xl scroll-mt-20 px-5 py-24 sm:py-32">
          <div data-reveal className="max-w-2xl">
            <Eyebrow n="01">Features</Eyebrow>
            <h2 className="text-sheen mt-5 text-[clamp(2rem,4.5vw,3.25rem)] leading-[1.05] font-semibold tracking-[-0.04em] text-balance">
              Built to be trusted with every app you own.
            </h2>
          </div>

          <div className="mt-12 grid gap-3 sm:grid-cols-2 lg:mt-16 lg:grid-cols-4 lg:auto-rows-[minmax(150px,auto)]">
            <article data-reveal onMouseMove={spotlight} className="tile p-6 sm:p-7 lg:col-span-2 lg:col-start-1 lg:row-span-2 lg:row-start-1">
              <div className="flex items-start justify-between gap-4">
                <h3 className="text-lg font-medium tracking-tight">Every source</h3>
                <span className="label-mono">01</span>
              </div>
              <p className="mt-2 max-w-md text-sm leading-relaxed text-secondary-text">
                Sparkle feeds, the Mac App Store, Homebrew, Electron update feeds and developers' own release pages. Camu reports the newest version any of them knows.
              </p>
              <SourcesVisual />
            </article>

            <article data-reveal style={{ "--d": "60ms" } as CSSProperties} onMouseMove={spotlight} className="tile p-6 sm:p-7 lg:col-span-2 lg:col-start-3 lg:row-span-2 lg:row-start-3">
              <div className="flex items-start justify-between gap-4">
                <h3 className="text-lg font-medium tracking-tight">Safe by default</h3>
                <span className="label-mono">03</span>
              </div>
              <p className="mt-2 max-w-md text-sm leading-relaxed text-secondary-text">
                A download only replaces an app if it's signed by the same developer. The old version is backed up first, and Roll back puts it back.
              </p>
              <SafetyVisual />
            </article>

            {/* On phones these six read as one compact list; on desktop they become bento tiles. */}
            <div className="divide-y divide-border overflow-hidden rounded-2xl border border-border bg-white/[0.015] sm:col-span-2 lg:contents">
              {minor.map((f, i) => (
                <article
                  key={f.n}
                  data-reveal
                  style={{ "--d": `${(i + 2) * 60}ms` } as CSSProperties}
                  onMouseMove={spotlight}
                  className={`flex gap-4 px-5 py-4 lg:tile lg:flex-col lg:gap-0 lg:p-6 ${f.place}`}
                >
                  <span className="label-mono pt-[3px] lg:order-first lg:mb-auto lg:self-end lg:pt-0">{f.n}</span>
                  <div className="min-w-0 lg:mt-6">
                    <h3 className="font-medium tracking-tight">{f.t}</h3>
                    <p className="mt-1 text-sm leading-relaxed text-secondary-text lg:mt-2">{f.d}</p>
                  </div>
                </article>
              ))}
            </div>
          </div>
        </section>

        {/* Showcase */}
        <section className="mx-auto max-w-6xl space-y-28 px-5 pb-24 sm:space-y-36 sm:pb-32">
          <div className="grid items-center gap-12 lg:grid-cols-[3fr_2fr] lg:gap-16">
            <div data-reveal className="relative">
              <div className="glow -inset-8" />
              <div className="frame">
                <img src={detail.url} width={2560} height={1600} className="block h-auto w-full"
                  alt="Camu app details panel showing installed and latest versions, each source's report, release notes and a Roll back button" />
              </div>
            </div>
            <div data-reveal style={{ "--d": "80ms" } as CSSProperties}>
              <Eyebrow n="02">Detail</Eyebrow>
              <h2 className="text-sheen mt-5 text-3xl leading-[1.08] font-semibold tracking-[-0.035em] text-balance sm:text-4xl">
                See where every version comes from
              </h2>
              <p className="mt-5 leading-relaxed text-secondary-text">
                Open any app for its installed and latest versions, what each source reported, the release notes and a one-step roll back.
              </p>
            </div>
          </div>

          <div className="grid items-center gap-12 lg:grid-cols-[2fr_3fr] lg:gap-16">
            <div data-reveal className="relative flex justify-center lg:order-2">
              <div className="glow inset-x-0 -inset-y-8" />
              <div className="frame w-full max-w-[380px]">
                <img src={menubar.url} width={760} height={960} className="block h-auto w-full"
                  alt="Camu menu bar panel listing pending updates with an Update all button" />
              </div>
            </div>
            <div data-reveal style={{ "--d": "80ms" } as CSSProperties} className="lg:order-1">
              <Eyebrow n="03">Menu bar</Eyebrow>
              <h2 className="text-sheen mt-5 text-3xl leading-[1.08] font-semibold tracking-[-0.035em] text-balance sm:text-4xl">
                Right there in your menu bar
              </h2>
              <p className="mt-5 leading-relaxed text-secondary-text">
                Click the menu bar icon for every pending update and Update all, without opening the window.
              </p>
            </div>
          </div>
        </section>

        {/* Install */}
        <section id="install" className="border-t border-border">
          <div className="mx-auto grid max-w-6xl scroll-mt-20 gap-12 px-5 py-24 sm:py-32 lg:grid-cols-[2fr_3fr] lg:gap-16">
            <div data-reveal>
              <Eyebrow n="04">Install</Eyebrow>
              <h2 className="text-sheen mt-5 text-[clamp(2rem,4.5vw,3.25rem)] leading-[1.05] font-semibold tracking-[-0.04em] text-balance">
                Up and running in a minute.
              </h2>
            </div>
            <ol className="relative">
              {[
                <>Download the Camu disk image and drag Camu to Applications.</>,
                <>
                  The first time you open it, macOS says it can't verify the developer. Open{" "}
                  <span className="chip">System Settings &gt; Privacy &amp; Security</span> and click{" "}
                  <span className="chip">Open Anyway</span>. You only do this once.
                </>,
                <>That's it. Camu checks for updates in the background and keeps itself up to date.</>,
              ].map((s, i, all) => (
                <li key={i} data-reveal style={{ "--d": `${i * 80}ms` } as CSSProperties} className="relative flex gap-5 pb-10 last:pb-0">
                  {i < all.length - 1 && (
                    <span aria-hidden="true" className="absolute top-9 bottom-1 left-[17px] w-px bg-gradient-to-b from-border-strong to-transparent" />
                  )}
                  <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-border-strong bg-surface font-mono text-xs text-download">
                    {String(i + 1).padStart(2, "0")}
                  </span>
                  <p className="min-w-0 pt-1.5 text-[17px] leading-relaxed text-foreground/90">{s}</p>
                </li>
              ))}
            </ol>
          </div>
        </section>

        {/* Final CTA */}
        <section className="relative overflow-hidden border-t border-border">
          <div className="grid-backdrop" />
          <div className="glow top-1/2 left-1/2 h-72 w-[42rem] max-w-[120%] -translate-x-1/2 -translate-y-1/2" />
          <div data-reveal className="relative mx-auto max-w-6xl px-5 py-28 text-center sm:py-36">
            <h2 className="text-sheen mx-auto max-w-3xl pb-1 text-[clamp(2.25rem,5.5vw,4rem)] leading-[1.04] font-semibold tracking-[-0.045em] text-balance">
              Every app on your Mac, up to date.
            </h2>
            <div className="mt-10 flex justify-center">
              <DownloadButton className="w-full max-w-xs sm:w-auto" />
            </div>
            <MetaLine />
          </div>
        </section>
      </main>

      <footer className="border-t border-border">
        <div className="mx-auto max-w-6xl px-5">
          <div className="flex flex-col gap-6 py-10 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-4">
              <img src={wordmark.url} alt="Camu" width={381} height={62} className="h-auto w-[72px]" />
              <span className="text-sm text-muted-foreground">Made for the Mac.</span>
            </div>
            <nav className="flex items-center gap-6">
              <a href={DMG} className={navLink}>Download</a>
              <a href="#features" className={navLink}>Features</a>
              <a href="#install" className={navLink}>Install</a>
            </nav>
          </div>
          <div className="label-mono flex flex-wrap justify-between gap-2 border-t border-border py-5">
            <span>© {new Date().getFullYear()} Camu</span>
            <span>Camu is in beta.</span>
          </div>
        </div>
      </footer>
    </div>
  );
}
