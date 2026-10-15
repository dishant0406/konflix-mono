/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // A self-contained server bundle, so the runtime image does not need the full
  // workspace tree.
  output: "standalone",
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
