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
      { protocol: "https", hostname: "s4.anilist.co" },
      { protocol: "https", hostname: "media.kitsu.app" },
    ],
  },
};

export default nextConfig;
