// Inter, one variable file for every weight the UI uses.
//
// The file is committed and loaded locally rather than fetched from Google at build time.
// next/font/google downloads the font during `next build`, which makes the build depend on
// fonts.googleapis.com: a blocked egress or a sandboxed builder fails the build, and the
// failure has nothing to do with the code.
import localFont from "next/font/local";

export const appFont = localFont({
  src: "../assets/fonts/inter-latin-wght-normal.woff2",
  weight: "100 900",
  style: "normal",
  display: "swap",
  variable: "--font-app",
});
