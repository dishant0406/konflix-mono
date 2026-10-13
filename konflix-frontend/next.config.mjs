/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // A self-contained server bundle, so the runtime image does not need the full
  // workspace tree.
  output: "standalone",
  images: {
    // Covers come from AniList and (with the Kitsu provider) Kitsu, over HTTPS.
    // Next only optimises hosts that are declared here.
    remotePatterns: [
      // Anime covers.
      { protocol: "https", hostname: "s4.anilist.co" },
      { protocol: "https", hostname: "media.kitsu.app" },
      // Series covers, from TVMaze.
      { protocol: "https", hostname: "static.tvmaze.com" },
      // Film covers and backdrops, from YTS.
      { protocol: "https", hostname: "yts.gg" },
      { protocol: "https", hostname: "yts.mx" },
    ],
  },
};

export default nextConfig;
