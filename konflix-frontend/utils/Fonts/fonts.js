// Two weights of one font, used across the UI.
//
// next/font/google replaces the deprecated @next/font package.
import { Roboto_Condensed } from 'next/font/google'

export const poppinsMedium = Roboto_Condensed({ weight: '400', subsets: ['latin'] })
export const poppinsBold = Roboto_Condensed({ weight: '700', subsets: ['latin'] })
