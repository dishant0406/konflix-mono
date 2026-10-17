import Link from "next/link";

import { albumPath, artistPath, type AlbumCard, type ArtistCard } from "@/lib/music";
import { cn } from "@/lib/utils";

import Bone from "./bone";
import Cover from "./cover";

/** A heading over a row that scrolls sideways, snapping to the cards. */
export function Shelf({ title, children, className }: { title: string; children: React.ReactNode; className?: string }) {
  return (
    <section className={cn("flex flex-col gap-3", className)}>
      <h2 className="gutter text-xl font-bold leading-7 tracking-tight">{title}</h2>
      <div className="no-scrollbar flex snap-x gap-1 overflow-x-auto scroll-px-2 px-2 md:scroll-px-7 md:px-7">{children}</div>
    </section>
  );
}

/** The card, shared with its skeleton so the two cannot drift apart. */
const CARD = "group flex w-40 shrink-0 snap-start flex-col gap-3 rounded-lg p-3 transition-colors hover:bg-white/[0.07] md:w-44";

export function AlbumCardLink({ album }: { album: AlbumCard }) {
  return (
    <Link href={albumPath(album.id)} className={CARD}>
      <Cover src={album.cover} sizes="176px" className="w-full" />
      <div className="min-w-0">
        <p className="truncate text-sm font-semibold leading-5 text-white">{album.title}</p>
        <p className="truncate text-[13px] leading-[18px] text-white/60">{[album.year, album.artist].filter(Boolean).join(" · ")}</p>
      </div>
    </Link>
  );
}

export function ArtistCardLink({ artist }: { artist: ArtistCard }) {
  return (
    <Link href={artistPath(artist.id)} className={CARD}>
      <Cover src={artist.cover} sizes="176px" round className="w-full" />
      <div className="min-w-0">
        <p className="truncate text-sm font-semibold leading-5 text-white">{artist.name}</p>
        <p className="truncate text-[13px] leading-[18px] text-white/60">Artist</p>
      </div>
    </Link>
  );
}

/**
 * What a shelf looks like before its cards arrive: the same heading, row and cards, with grey
 * blocks where the pictures and the two lines of text go, so nothing moves when they come.
 */
export function ShelfSkeleton({ round }: { round?: boolean }) {
  return (
    <section className="flex flex-col gap-3" aria-hidden>
      <div className="gutter flex h-7 items-center">
        <Bone className="h-5 w-40" />
      </div>
      <div className="no-scrollbar flex gap-1 overflow-hidden px-2 md:px-7">
        {Array.from({ length: 10 }, (_, i) => (
          <div key={i} className="flex w-40 shrink-0 flex-col gap-3 p-3 md:w-44">
            <Bone className={cn("aspect-square w-full", round && "rounded-full")} />
            <div className="min-w-0">
              <div className="flex h-5 items-center">
                <Bone className="h-3.5 w-3/4" />
              </div>
              <div className="flex h-[18px] items-center">
                <Bone className="h-3 w-1/2" />
              </div>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
