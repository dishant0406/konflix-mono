import Link from "next/link";

import Container from "@/components/container";
import { SITE } from "@/constants/site";

export default function Footer() {
  return (
    <footer className="mt-16 w-full border-t border-border bg-card/40 py-8">
      <Container className="flex flex-col items-center gap-4 text-center">
        <span className="bg-gradient-to-r from-foreground to-primary bg-clip-text text-2xl font-black tracking-widest text-transparent">
          KONFLIX
        </span>
        <p className="max-w-2xl text-xs text-muted-foreground">
          Konflix stores no media on its own servers. {SITE.attribution} Playback links
          are resolved from public sources at the moment you press play.
        </p>
        <nav className="flex items-center gap-6 text-xs text-muted-foreground">
          <Link href="/" className="hover:text-foreground">
            Home
          </Link>
          <Link href="/search?sort=popularity" className="hover:text-foreground">
            Browse
          </Link>
        </nav>
        <p className="text-xs text-muted-foreground/70">&copy; Konflix</p>
      </Container>
    </footer>
  );
}
