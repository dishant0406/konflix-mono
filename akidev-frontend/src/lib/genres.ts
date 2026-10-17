// The genres the music home offers. Each has a motif (public/genres/<slug>.svg, drawn as a shape the
// tile paints in its accent colour) over a dark tint of that colour. The desktop app has the same
// list and the same files (desktop/app/src/music_home.rs).

export interface Genre {
  name: string;
  slug: string;
  /** The motif's colour. */
  accent: string;
  /** The card's own colour: the accent, mostly black. */
  tint: string;
}

export const GENRES: Genre[] = [
  { name: "Pop", slug: "pop", accent: "#e0809a", tint: "#2b1a20" },
  { name: "Hip-Hop", slug: "hip-hop", accent: "#d9ab4c", tint: "#282114" },
  { name: "Rock", slug: "rock", accent: "#e0644f", tint: "#2b1815" },
  { name: "Bollywood", slug: "bollywood", accent: "#e8924a", tint: "#2b1e14" },
  { name: "Electronic", slug: "electronic", accent: "#5f9fe0", tint: "#141f2c" },
  { name: "R&B", slug: "rnb", accent: "#a083e0", tint: "#1d182b" },
  { name: "Indie", slug: "indie", accent: "#62b78f", tint: "#14251e" },
  { name: "Jazz", slug: "jazz", accent: "#cf935c", tint: "#261c14" },
  { name: "K-Pop", slug: "kpop", accent: "#e07fbb", tint: "#2b1724" },
  { name: "Lo-fi", slug: "lofi", accent: "#7fb0b0", tint: "#172424" },
  { name: "Classical", slug: "classical", accent: "#c9bb70", tint: "#25231a" },
  { name: "Metal", slug: "metal", accent: "#a8a8a8", tint: "#1d1d1d" },
];
