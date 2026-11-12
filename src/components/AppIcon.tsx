import { useEffect, useState } from "react";

import { appIcon, type InstalledApp } from "../lib/native";

/** Icons already converted this session, by app path. */
const cache = new Map<string, string | null>();

/** The app's own icon, or a monogram tile while it loads or if it has none. */
export function AppIcon({ app }: { app: InstalledApp }) {
  const [src, setSrc] = useState(() => cache.get(app.path) ?? null);

  useEffect(() => {
    if (cache.has(app.path) || !app.iconFile) return;
    let live = true;
    appIcon(app.path, app.iconFile)
      .then((url) => {
        cache.set(app.path, url);
        if (live) setSrc(url);
      })
      .catch(() => cache.set(app.path, null));
    return () => {
      live = false;
    };
  }, [app.path, app.iconFile]);

  if (src) return <img className="icon" src={src} alt="" draggable={false} />;
  return (
    <span className="icon monogram" aria-hidden>
      {app.name.slice(0, 1).toUpperCase()}
    </span>
  );
}
