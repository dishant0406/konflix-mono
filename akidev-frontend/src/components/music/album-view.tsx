"use client";

import Link from "next/link";

import { ErrorState } from "@/components/states";
import { artistPath } from "@/lib/music";
import { useAlbum } from "@/query/use-music";
import { useTint } from "@/query/use-tint";

import Actions, { ActionsSkeleton } from "./actions";
import Hero, { HeroSkeleton } from "./hero";
import TrackList, { TrackListSkeleton } from "./track-list";

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
      <>
        <HeroSkeleton />
        <div className="relative">
          <ActionsSkeleton />
          <div className="px-1 md:px-6">
            <TrackListSkeleton variant="table" rows={10} />
          </div>
        </div>
      </>
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
