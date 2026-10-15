// Calls the API from the server that renders a page, so the page arrives with its data in it.
//
// Inside the stack the API is reached on the internal network (API_INTERNAL_URL), which is a hop
// on the same machine rather than a trip across the internet to the public address. A failure
// answers null and the page falls back to asking from the browser, as it did before.

import { headers } from "next/headers";

const BASE = (process.env.API_INTERNAL_URL || process.env.NEXT_PUBLIC_API_URL || "http://localhost:9000").replace(/\/+$/, "");

/** `path` answered as JSON, kept by the framework for `revalidate` seconds, or null when it could not be. */
export async function serverGet<T>(path: string, revalidate: number): Promise<T | null> {
  try {
    const response = await fetch(`${BASE}${path}`, { next: { revalidate }, signal: AbortSignal.timeout(8000) });
    return response.ok ? ((await response.json()) as T) : null;
  } catch {
    return null;
  }
}

/**
 * Whether this request is the router moving between pages in a browser that is already open, as
 * opposed to a document being loaded.
 *
 * A document is rendered with its data in it, so the first paint does not wait for scripts and
 * then for a request of their own. A move between pages does not need that: the browser has the
 * scripts, and asks the API itself (and often already has the answer, from hovering the link).
 * Waiting for the server's data first would hold the screen on the old page, or on a loader, for a
 * trip to the server; answering at once lets the new page appear in place and fill in.
 */
export async function isClientNavigation(): Promise<boolean> {
  return (await headers()).get("rsc") === "1";
}
