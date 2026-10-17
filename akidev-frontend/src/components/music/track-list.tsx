"use client";

import { Clock, Pause, Play } from "lucide-react";
import Link from "next/link";

import { albumPath, artistPath, type Track } from "@/lib/music";
import { cn } from "@/lib/utils";
import { useMusicPlayer } from "@/providers/music-player";

import Cover from "./cover";
import Equaliser from "./equaliser";

/**
 * `table` is an album: numbered rows under a header, no covers (they are all the same cover).
 * `list` is anything mixed (search, an artist's songs): a small cover leads each row.
 * Pressing a row plays it and queues the rest of the list after it.
 */
export default function TrackList({ tracks, variant }: { tracks: Track[]; variant: "table" | "list" }) {
  const player = useMusicPlayer();
  const table = variant === "table";
  const columns = table
    ? "grid-cols-[2rem_minmax(0,1fr)_3rem] md:grid-cols-[2rem_minmax(0,1fr)_3rem]"
    : "grid-cols-[2.5rem_minmax(0,1fr)_3rem] md:grid-cols-[2.5rem_minmax(0,6fr)_minmax(0,4fr)_3rem]";

  return (
    <div role="table" className="flex flex-col">
      {table && (
        <div className={cn("mb-1 grid h-9 items-center gap-4 border-b border-white/[0.08] px-4 text-xs text-white/50", columns)}>
          <span className="text-center">#</span>
          <span>Title</span>
          <Clock className="ml-auto size-4" />
        </div>
      )}
      {tracks.map((track, i) => {
        const isCurrent = player.current?.id === track.id;
        const isPlaying = isCurrent && player.playing;
        const press = () => (isCurrent ? player.toggle() : player.play(tracks, i));
        return (
          <div
            key={`${track.id}-${i}`}
            role="row"
            onDoubleClick={press}
            className={cn("group grid h-14 items-center gap-4 rounded-md px-4 transition-colors hover:bg-white/[0.07]", columns, isCurrent && "bg-white/[0.04]")}
          >
            <button
              type="button"
              onClick={press}
              aria-label={isPlaying ? `Pause ${track.title}` : `Play ${track.title}`}
              className="relative -mx-2 flex h-10 cursor-pointer items-center justify-center"
            >
              {table ? (
                <>
                  <span className={cn("text-sm tabular-nums text-white/60 group-hover:hidden", isCurrent && "hidden")}>{i + 1}</span>
                  {isCurrent && <span className="group-hover:hidden text-white"><Equaliser paused={!isPlaying} /></span>}
                  <span className="hidden group-hover:block">{isPlaying ? <Pause className="size-4 fill-current" /> : <Play className="size-4 fill-current" />}</span>
                </>
              ) : (
                <>
                  <Cover src={track.cover} sizes="40px" className="size-10 rounded" />
                  <span
                    className={cn(
                      "absolute inset-0 items-center justify-center rounded bg-black/55",
                      isCurrent ? "flex" : "hidden group-hover:flex",
                    )}
                  >
                    {isPlaying ? <Pause className="size-4 fill-current" /> : <Play className="size-4 fill-current" />}
                  </span>
                </>
              )}
            </button>

            <div className="min-w-0">
              <p className={cn("truncate text-sm font-medium", isCurrent ? "text-white" : "text-white/90")}>
                {track.title}
                {track.explicit && <span className="ml-2 rounded-sm bg-white/20 px-1 py-px text-[10px] font-bold text-white/70">E</span>}
              </p>
              <p className="truncate text-[13px] text-white/60">
                {track.artists.map((artist, n) => (
                  <span key={`${artist.name}-${n}`}>
                    {n > 0 && ", "}
                    {artist.id ? (
                      <Link href={artistPath(artist.id)} className="hover:text-white hover:underline">
                        {artist.name}
                      </Link>
                    ) : (
                      artist.name
                    )}
                  </span>
                ))}
              </p>
            </div>

            {!table && (
              <div className="hidden min-w-0 md:block">
                {track.album && (
                  <Link
                    href={track.album.id ? albumPath(track.album.id) : "#"}
                    className="block truncate text-[13px] text-white/60 hover:text-white hover:underline"
                  >
                    {track.album.name}
                  </Link>
                )}
              </div>
            )}
            <span className="text-right text-[13px] tabular-nums text-white/60">{track.duration}</span>
          </div>
        );
      })}
    </div>
  );
}
