"use client";

import { ErrorState } from "@/components/states";
import { useArtist } from "@/query/use-music";
import { useTint } from "@/query/use-tint";

import Actions, { ActionsSkeleton } from "./actions";
import Bone from "./bone";
import { AlbumCardLink, ArtistCardLink, Shelf, ShelfSkeleton } from "./cards";
import Hero, { HeroSkeleton } from "./hero";
import TrackList, { TrackListSkeleton } from "./track-list";

export default function ArtistView({ id }: { id: string }) {
  const artist = useArtist(id);
  const data = artist.data;
  const tint = useTint(data?.cover);

  if (artist.isError) {
    return (
      <div className="pt-20">
        <ErrorState title="That artist could not be loaded" detail="Try again in a moment." onRetry={() => artist.refetch()} />
      </div>
    );
  }
  if (!data) {
    return (
      <>
        <HeroSkeleton round />
        <div className="relative flex flex-col gap-8 pb-8">
          <div>
            <ActionsSkeleton />
            <section className="flex flex-col gap-1 px-1 md:px-6">
              <div className="flex h-9 items-start px-2 pb-2">
                <div className="flex h-7 items-center">
                  <Bone className="h-5 w-20" />
                </div>
              </div>
              <TrackListSkeleton variant="list" rows={5} />
            </section>
          </div>
          <ShelfSkeleton />
          <ShelfSkeleton />
        </div>
      </>
    );
  }

  return (
    <>
      <Hero tint={tint} cover={data.cover} round kind="Artist" title={data.name}>
        {data.subscribers && <p className="text-sm text-white/70">{data.subscribers} subscribers</p>}
      </Hero>
      <div className="relative flex flex-col gap-8 pb-8" style={{ background: `linear-gradient(to bottom, rgb(${tint} / 0.12), transparent 12rem)` }}>
        <div>
          <Actions tracks={data.songs} />
          {data.songs.length > 0 && (
            <section className="flex flex-col gap-1 px-1 md:px-6">
              <h2 className="px-2 pb-2 text-xl font-bold leading-7 tracking-tight">Popular</h2>
              <TrackList tracks={data.songs} variant="list" />
            </section>
          )}
        </div>
        {data.albums.length > 0 && (
          <Shelf title="Albums">
            {data.albums.map((album) => (
              <AlbumCardLink key={album.id} album={album} />
            ))}
          </Shelf>
        )}
        {data.singles.length > 0 && (
          <Shelf title="Singles">
            {data.singles.map((album) => (
              <AlbumCardLink key={album.id} album={album} />
            ))}
          </Shelf>
        )}
        {data.related.length > 0 && (
          <Shelf title="Fans also like">
            {data.related.map((related) => (
              <ArtistCardLink key={related.id} artist={{ ...related, cover: related.cover }} />
            ))}
          </Shelf>
        )}
        {data.description && <p className="max-w-3xl whitespace-pre-line gutter text-sm leading-6 text-white/60">{data.description}</p>}
      </div>
    </>
  );
}
