// Where next/image gets a picture at the width it asks for.
//
// The server resizes each poster and banner once per width and answers with an immutable file
// (`/v1/img`), which the edge and the browser keep for good. The framework's own optimiser is not
// used: it answers `Vary: Accept`, which Cloudflare will not cache, so every picture made the trip
// to the server on every view. The widths asked for here are the ones the server makes, see
// `deviceSizes` and `imageSizes` in next.config.mjs.

/**
 * Changes whenever the server starts making pictures differently, which gives them new addresses.
 * The edge and the browsers keep a picture for good, so a fix to how one is made reaches nobody
 * until its address changes. The server ignores it.
 */
const MADE = 2;

const BASE = (process.env.NEXT_PUBLIC_API_URL || "http://localhost:9000").replace(/\/+$/, "");

export default function imageLoader({ src, width, quality }: { src: string; width: number; quality?: number }): string {
  return `${BASE}/v1/img?u=${encodeURIComponent(src)}&w=${width}&q=${quality ?? 75}&v=${MADE}`;
}
