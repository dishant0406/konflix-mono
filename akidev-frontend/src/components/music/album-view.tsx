"use client";

import Link from "next/link";

import { ErrorState, Skeleton } from "@/components/states";
import { artistPath } from "@/lib/music";
import { useAlbum } from "@/query/use-music";
import { useTint } from "@/query/use-tint";

import Actions from "./actions";
import Hero from "./hero";
import TrackList from "./track-list";

export default function AlbumView({ id }: { id: string }) {
  const album = useAlbum(id);
  const data = album.data;
  const tint = useTint(data?.cover);

  if (album.isError) {
    return (
      <div className="pt-20">
        <ErrorState title="That album could not be loaded" detail="It may have been removed." onRetry={() => album.refetch()} />
      </div>
    );
  }
  if (!data) {
    return (
      <div className="flex flex-col gap-3 pt-20 gutter">
        <Skeleton className="h-56 bg-white/10" />
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton key={i} className="h-12 bg-white/10" />
        ))}
      </div>
    );
  }

  return (
    <>
      <Hero tint={tint} cover={data.cover} kind={data.kind ?? "Album"} title={data.title}>
        <p className="flex flex-wrap items-center gap-x-1.5 text-sm text-white/70">
          {data.artists.map((artist, n) => (
            <span key={`${artist.name}-${n}`}>
              {n > 0 && ", "}
              {artist.id ? (
                <Link href={artistPath(artist.id)} className="font-semibold text-white hover:underline">
                  {artist.name}
                </Link>
              ) : (
                <span className="font-semibold text-white">{artist.name}</span>
              )}
            </span>
          ))}
          <span>· {data.year}</span>
          <span>· {data.tracks.length} songs, {data.duration}</span>
        </p>
      </Hero>
      <div className="relative" style={{ background: `linear-gradient(to bottom, rgb(${tint} / 0.12), transparent 12rem)` }}>
        <Actions tracks={data.tracks} />
        <div className="px-1 md:px-6">
          <TrackList tracks={data.tracks} variant="table" />
        </div>
      </div>
    </>
  );
}
