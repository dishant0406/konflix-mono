// Two faces, both committed and loaded locally (next/font/google would make `next build` depend on
// fonts.googleapis.com).
//
// Hanken Grotesk carries the interface: a calm grotesk with an open, even texture, one variable
// file for every weight the UI uses. Shippori Mincho is the title face, a Japanese film-title
// mincho; only the Latin letters, common punctuation and 秋 (the mark) are kept, so it costs 60 KB
// instead of 8 MB. Both are SIL Open Font License.
import localFont from "next/font/local";

export const appFont = localFont({
  src: "../assets/fonts/hanken-grotesk-wght.woff2",
  weight: "100 900",
  style: "normal",
  display: "swap",
  variable: "--font-app",
});

export const titleFont = localFont({
  src: [
    { path: "../assets/fonts/shippori-mincho-400.woff2", weight: "400", style: "normal" },
    { path: "../assets/fonts/shippori-mincho-600.woff2", weight: "600", style: "normal" },
  ],
  display: "swap",
  variable: "--font-title",
});
