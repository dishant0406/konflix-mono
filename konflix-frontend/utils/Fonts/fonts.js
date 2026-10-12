// Two weights of one font, used across the UI.
//
// The files are committed and loaded locally rather than fetched from Google at
// build time. next/font/google downloads the font during `next build`, which makes
// the build depend on fonts.googleapis.com: a network hiccup, a blocked egress, or a
// sandboxed builder fails the build, and the failure has nothing to do with the code.
// The two woff2 files here are the latin subsets of Roboto Condensed 400 and 700.
import localFont from 'next/font/local'

const robotoCondensed = localFont({
  src: [
    { path: './roboto-condensed-latin-400.woff2', weight: '400', style: 'normal' },
    { path: './roboto-condensed-latin-700.woff2', weight: '700', style: 'normal' },
  ],
  display: 'swap',
})

// Kept as two names because every call site imports them separately.
export const poppinsMedium = robotoCondensed
export const poppinsBold = robotoCondensed
