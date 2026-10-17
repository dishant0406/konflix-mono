import Link from "next/link";

import { albumPath, artistPath, type AlbumCard, type ArtistCard } from "@/lib/music";
import { cn } from "@/lib/utils";

import Cover from "./cover";

/** A heading over a row that scrolls sideways, snapping to the cards. */
export function Shelf({ title, children, className }: { title: string; children: React.ReactNode; className?: string }) {
  return (
    <section className={cn("flex flex-col gap-3", className)}>
      <h2 className="gutter text-xl font-bold tracking-tight">{title}</h2>
      <div className="no-scrollbar flex snap-x gap-1 overflow-x-auto scroll-px-2 px-2 md:scroll-px-7 md:px-7">{children}</div>
    </section>
  );
}

const CARD = "group flex w-40 shrink-0 snap-start flex-col gap-3 rounded-lg p-3 transition-colors hover:bg-white/[0.07] md:w-44";

export function AlbumCardLink({ album }: { album: AlbumCard }) {
  return (
    <Link href={albumPath(album.id)} className={CARD}>
      <Cover src={album.cover} sizes="176px" className="w-full" />
      <div className="min-w-0">
        <p className="truncate text-sm font-semibold text-white">{album.title}</p>
        <p className="truncate text-[13px] text-white/60">{[album.year, album.artist].filter(Boolean).join(" · ")}</p>
      </div>
    </Link>
  );
}

export function ArtistCardLink({ artist }: { artist: ArtistCard }) {
  return (
    <Link href={artistPath(artist.id)} className={CARD}>
      <Cover src={artist.cover} sizes="176px" round className="w-full" />
      <div className="min-w-0">
        <p className="truncate text-sm font-semibold text-white">{artist.name}</p>
        <p className="truncate text-[13px] text-white/60">Artist</p>
      </div>
    </Link>
  );
}
