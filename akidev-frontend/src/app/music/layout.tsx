import type { Metadata } from "next";

export const metadata: Metadata = { title: "Music | Akidev", description: "Search and stream music." };

export default function MusicLayout({ children }: { children: React.ReactNode }) {
  return children;
}
