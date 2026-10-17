import type { Config } from "tailwindcss";

// The colours are the desktop app's, as RGB channels so Tailwind's /alpha modifiers work.
// See desktop/app/src/design/tokens.rs: a near-black canvas with grey steps for surfaces, so
// the artwork is the colour on screen. Amber is the one accent.
const token = (name: string) => `rgb(var(--${name}) / <alpha-value>)`;

const config: Config = {
	content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
	theme: {
		extend: {
			colors: {
				canvas: token("canvas"),
				surface: token("surface"),
				raised: token("raised"),
				line: token("line"),
				ink: token("ink"),
				dim: token("dim"),
				faint: token("faint"),
				accent: token("accent"),
				danger: token("danger"),
				mbg: token("mbg"),
				mpanel: token("mpanel"),
			},
			fontFamily: {
				sans: ["var(--font-app)", "system-ui", "sans-serif"],
			},
			keyframes: {
				rise: { from: { opacity: "0", transform: "translateY(6px)" }, to: { opacity: "1", transform: "none" } },
				fade: { from: { opacity: "0" }, to: { opacity: "1" } },
				pulse: { "0%,100%": { opacity: "1" }, "50%": { opacity: "0.55" } },
				pop: { from: { opacity: "0", transform: "translateY(-4px) scale(0.97)" }, to: { opacity: "1", transform: "none" } },
			},
			animation: {
				rise: "rise 0.35s ease-out both",
				fade: "fade 0.7s cubic-bezier(0.22,1,0.36,1) both",
				pulse: "pulse 1.6s ease-in-out infinite",
				pop: "pop 0.16s cubic-bezier(0.22,1,0.36,1) both",
			},
		}
	},
	plugins: [],
};
export default config;
