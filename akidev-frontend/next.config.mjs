/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // A self-contained server bundle, so the runtime image does not need the full
  // workspace tree.
  output: "standalone",
  images: {
    // Covers come from AniList, Kitsu, TVMaze and Cinemeta, over HTTPS. Next only
    // optimises hosts that are declared here, and an undeclared host fails at runtime
    // while the build still passes.
    remotePatterns: [
      // Anime covers.
      { protocol: "https", hostname: "s4.anilist.co" },
      { protocol: "https", hostname: "media.kitsu.app" },
      // Series covers, from TVMaze.
      { protocol: "https", hostname: "static.tvmaze.com" },
      // Film covers, backdrops and logos, from Cinemeta. Three hosts: the API serves
      // some posters from Amazon directly and the rest from metahub, which redirects
      // `images.` to `live.`.
      { protocol: "https", hostname: "images.metahub.space" },
      { protocol: "https", hostname: "live.metahub.space" },
      { protocol: "https", hostname: "m.media-amazon.com" },
      // Cast photos, which Cinemeta supplies as TMDB paths.
      { protocol: "https", hostname: "image.tmdb.org" },
      // Film covers and backdrops, from YTS, which remains a fallback provider.
      { protocol: "https", hostname: "yts.gg" },
      { protocol: "https", hostname: "yts.mx" },
    ],
  },
};

export default nextConfig;
