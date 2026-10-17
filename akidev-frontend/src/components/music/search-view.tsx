"use client";

import { Play } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import { EmptyState, ErrorState, Skeleton } from "@/components/states";
import { albumPath, artistPath, type MusicSearch } from "@/lib/music";
import { cn } from "@/lib/utils";
import { useMusicSearch } from "@/query/use-music";
import { useMusicPlayer } from "@/providers/music-player";

import { AlbumCardLink, ArtistCardLink, Shelf } from "./cards";
import Cover from "./cover";
import TrackList from "./track-list";

const FILTERS = ["All", "Songs", "Albums", "Artists"] as const;
type Filter = (typeof FILTERS)[number];

/** The result worth leading with: the artist when the search is their name, else the first song. */
function TopResult({ found, term }: { found: MusicSearch; term: string }) {
  const player = useMusicPlayer();
  const artist = found.artists[0];
  const song = found.songs[0];
  const asArtist = artist && (artist.name.toLowerCase().includes(term.toLowerCase()) || !song);
  if (!asArtist && !song) return null;
  const href = asArtist ? artistPath(artist.id) : song.album?.id ? albumPath(song.album.id) : "#";
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-xl font-bold tracking-tight">Top result</h2>
      <div className="group relative flex flex-col gap-4 rounded-lg bg-white/[0.06] p-5 transition-colors hover:bg-white/10">
        <Link href={href} aria-label={asArtist ? artist.name : song.title} className="absolute inset-0 rounded-lg" />
        <Cover src={asArtist ? artist.cover : song.cover} sizes="96px" round={asArtist} className="size-24" />
        <div className="min-w-0">
          <p className="truncate text-3xl font-extrabold tracking-tight">{asArtist ? artist.name : song.title}</p>
          <p className="truncate text-sm text-white/60">
            {asArtist ? "Artist" : `Song · ${song.artists.map((a) => a.name).join(", ")}`}
          </p>
        </div>
        {!asArtist && (
          <button
            type="button"
            aria-label={`Play ${song.title}`}
            onClick={() => player.play(found.songs, 0)}
            className="absolute bottom-5 right-5 flex size-12 cursor-pointer items-center justify-center rounded-full bg-white text-black opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
          >
            <Play className="size-5 fill-current" />
          </button>
        )}
      </div>
    </section>
  );
}

export default function SearchView({ term }: { term: string }) {
  const [filter, setFilter] = useState<Filter>("All");
  const search = useMusicSearch(term);
  const found = search.data;

  if (!term) {
    return null;
  }
  if (search.isError && !found) {
    return <ErrorState title="Search is not answering" detail="Try again in a moment." onRetry={() => search.refetch()} />;
  }
  if (!found) {
    return (
      <div className="flex flex-col gap-3 gutter">
        {Array.from({ length: 7 }, (_, i) => (
          <Skeleton key={i} className="h-14 bg-white/10" />
        ))}
      </div>
    );
  }
  if (found.songs.length + found.albums.length + found.artists.length === 0) {
    return <EmptyState title={`No results for "${term}"`} detail="Check the spelling, or try the artist's name." />;
  }

  const all = filter === "All";
  return (
    <div className="flex flex-col gap-8 pb-8">
      <div className="flex gap-2 gutter">
        {FILTERS.map((name) => (
          <button
            key={name}
            type="button"
            onClick={() => setFilter(name)}
            aria-pressed={filter === name}
            className={cn(
              "h-8 cursor-pointer rounded-full px-4 text-sm font-medium transition-colors",
              filter === name ? "bg-white text-black" : "bg-white/10 text-white hover:bg-white/[0.16]",
            )}
          >
            {name}
          </button>
        ))}
      </div>

      {all && (
        <div className="grid gap-8 px-4 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] md:px-8">
          <TopResult found={found} term={term} />
          {found.songs.length > 0 && (
            <section className="flex min-w-0 flex-col gap-3">
              <h2 className="text-xl font-bold tracking-tight">Songs</h2>
              <div className="-mx-4">
                <TrackList tracks={found.songs.slice(0, 4)} variant="list" />
              </div>
            </section>
          )}
        </div>
      )}
      {filter === "Songs" && (
        <div className="px-1 md:px-6">
          <TrackList tracks={found.songs} variant="list" />
        </div>
      )}
      {(all || filter === "Artists") && found.artists.length > 0 && (
        <Shelf title="Artists">
          {found.artists.map((artist) => (
            <ArtistCardLink key={artist.id} artist={artist} />
          ))}
        </Shelf>
      )}
      {(all || filter === "Albums") && found.albums.length > 0 && (
        <Shelf title="Albums">
          {found.albums.map((album) => (
            <AlbumCardLink key={album.id} album={album} />
          ))}
        </Shelf>
      )}
    </div>
  );
}
