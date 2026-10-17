"use client";

import Link from "next/link";

import { ErrorState, Skeleton } from "@/components/states";
import { GENRES } from "@/lib/genres";
import { musicSearchPath } from "@/lib/music";
import { useMusicHome } from "@/query/use-music";

import { AlbumCardLink, ArtistCardLink, Shelf } from "./cards";

export default function HomeView() {
  const home = useMusicHome();

  return (
    <div className="flex flex-col gap-10">
      {home.isError && !home.data ? (
        <ErrorState title="Music is not answering" detail="Try again in a moment." onRetry={() => home.refetch()} />
      ) : !home.data ? (
        <div className="gutter flex gap-3">
          {Array.from({ length: 6 }, (_, i) => (
            <Skeleton key={i} className="h-56 w-44 shrink-0 bg-white/10" />
          ))}
        </div>
      ) : (
        <>
          {home.data.artists.length > 0 && (
            <Shelf title="Popular artists">
              {home.data.artists.map((artist) => (
                <ArtistCardLink key={artist.id} artist={artist} />
              ))}
            </Shelf>
          )}
          {home.data.albums.length > 0 && (
            <Shelf title="Albums to start with">
              {home.data.albums.map((album) => (
                <AlbumCardLink key={album.id} album={album} />
              ))}
            </Shelf>
          )}
        </>
      )}

      <section className="flex flex-col gap-3 gutter">
        <h2 className="text-xl font-bold tracking-tight">Browse</h2>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
          {GENRES.map(({ name, slug, accent, tint }) => (
            <Link
              key={slug}
              href={musicSearchPath(`${name} music`)}
              style={{ backgroundColor: tint }}
              className="group relative h-28 overflow-hidden rounded-lg p-4 outline-none ring-1 ring-inset ring-white/[0.06] transition-colors hover:ring-white/[0.16] focus-visible:ring-white/40"
            >
              <span className="relative text-base font-bold tracking-tight text-white">{name}</span>
              {/* The motif is a shape painted in the accent colour, set off the corner so the card crops it. */}
              <span
                aria-hidden
                className="absolute -bottom-5 -right-4 size-28 opacity-90 transition-opacity group-hover:opacity-100"
                style={{
                  backgroundColor: accent,
                  maskImage: `url(/genres/${slug}.svg)`,
                  WebkitMaskImage: `url(/genres/${slug}.svg)`,
                  maskSize: "contain",
                  WebkitMaskSize: "contain",
                  maskRepeat: "no-repeat",
                  WebkitMaskRepeat: "no-repeat",
                }}
              />
            </Link>
          ))}
        </div>
      </section>
    </div>
  );
}
