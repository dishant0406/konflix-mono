// Two weights of one font, used across the UI.
//
// The files are committed and loaded locally rather than fetched from Google at
// build time. next/font/google downloads the font during `next build`, which makes
// the build depend on fonts.googleapis.com: a blocked egress or a sandboxed builder
// fails the build, and the failure has nothing to do with the code.
import localFont from "next/font/local";

export const appFont = localFont({
  src: [
    { path: "../assets/fonts/roboto-condensed-latin-400.woff2", weight: "400", style: "normal" },
    { path: "../assets/fonts/roboto-condensed-latin-700.woff2", weight: "700", style: "normal" },
  ],
  display: "swap",
  variable: "--font-app",
});
