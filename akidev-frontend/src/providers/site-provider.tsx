"use client";

import { createContext, useContext, type ReactNode } from "react";

import type { SiteConfig } from "@/constants/site";

const SiteContext = createContext<SiteConfig | null>(null);

/**
 * Carries the request's site down to client components.
 *
 * The layout resolves the site from the host on the server and passes it here, so the
 * client reads one value from context instead of every component re-deriving it. The
 * alternative - reading the host in the browser - would break the first paint, because
 * the server-rendered HTML has to match.
 */
export default function SiteProvider({ site, children }: { site: SiteConfig; children: ReactNode }) {
  return <SiteContext.Provider value={site}>{children}</SiteContext.Provider>;
}

export function useSite(): SiteConfig {
  const site = useContext(SiteContext);
  if (!site) {
    throw new Error("useSite was called outside a SiteProvider.");
  }
  return site;
}
