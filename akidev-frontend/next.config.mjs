/** @type {import('next').NextConfig} */
// What the edge may keep of a page. The pages are the same for everyone (what is personal, the
// viewer's history, is read in the browser), so Cloudflare serves them from near the visitor for
// half a minute and refreshes them in the background for the minute after. A short life because a
// deploy changes the scripts a page names, and an old page must not outlive them for long.
const pageCache = [{ key: "Cache-Control", value: "public, max-age=0, s-maxage=30, stale-while-revalidate=60" }];

const nextConfig = {
  reactStrictMode: true,
  experimental: {
    // A page the router has fetched is kept for half a minute, so going back and forth between
    // two pages asks the server once, and a link that has been hovered is already there on click.
    staleTimes: { dynamic: 30, static: 180 },
  },
  // A self-contained server bundle, so the runtime image does not need the full
  // workspace tree.
  output: "standalone",
  async headers() {
    return [
      { source: "/", headers: pageCache },
      { source: "/:kind(anime|series|films)", headers: pageCache },
      { source: "/title/:key", headers: pageCache },
    ];
  },
  images: {
    // Pictures come from the API, which resizes them (see src/lib/image-loader.ts). These are the
    // widths it makes, so the browser asks for the ones that exist and the edge shares them
    // between visitors.
    loader: "custom",
    loaderFile: "./src/lib/image-loader.ts",
    deviceSizes: [640, 828, 1080, 1280, 1600, 1920],
    imageSizes: [96, 176, 256, 352],
  },
};

export default nextConfig;
