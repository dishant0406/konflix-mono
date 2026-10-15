// Calls the API from the server that renders a page, so the page arrives with its data in it.
//
// Inside the stack the API is reached on the internal network (API_INTERNAL_URL), which is a hop
// on the same machine rather than a trip across the internet to the public address. A failure
// answers null and the page falls back to asking from the browser, as it did before.

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
