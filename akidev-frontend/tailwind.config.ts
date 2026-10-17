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
				// Three fills for anything lifted off the canvas, so every hover and panel shares them.
				wash: "rgb(var(--ink) / 0.06)",
				"wash-2": "rgb(var(--ink) / 0.1)",
				"wash-3": "rgb(var(--ink) / 0.16)",
				mbg: token("mbg"),
				mpanel: token("mpanel"),
			},
			// One scale: 4 for badges, 8 for buttons, 12 for tiles and panels, 16 for popups.
			borderRadius: { sm: "4px", DEFAULT: "4px", md: "8px", lg: "12px", xl: "16px", "2xl": "16px" },
			fontFamily: {
				sans: ["var(--font-app)", "system-ui", "sans-serif"],
				title: ["var(--font-title)", "Georgia", "serif"],
			},
			// Apple's default spring-like curve: quick out, soft settle, no overshoot.
			transitionTimingFunction: { spring: "cubic-bezier(0.32,0.72,0,1)" },
			keyframes: {
				slide: { from: { opacity: "0", transform: "translateX(24px)" }, to: { opacity: "1", transform: "none" } },
				rise: { from: { opacity: "0", transform: "translateY(6px)" }, to: { opacity: "1", transform: "none" } },
				fade: { from: { opacity: "0" }, to: { opacity: "1" } },
				pulse: { "0%,100%": { opacity: "1" }, "50%": { opacity: "0.55" } },
				pop: { from: { opacity: "0", transform: "translateY(-4px) scale(0.97)" }, to: { opacity: "1", transform: "none" } },
			},
			animation: {
				slide: "slide 0.35s cubic-bezier(0.32,0.72,0,1) both",
				rise: "rise 0.35s ease-out both",
				fade: "fade 0.7s cubic-bezier(0.22,1,0.36,1) both",
				pulse: "pulse 1.6s ease-in-out infinite",
				pop: "pop 0.16s cubic-bezier(0.32,0.72,0,1) both",
			},
		}
	},
	plugins: [],
};
export default config;
