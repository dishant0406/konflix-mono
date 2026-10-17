"use client";

import { Play, Shuffle } from "lucide-react";

import type { Track } from "@/lib/music";
import { cn } from "@/lib/utils";
import { useMusicPlayer } from "@/providers/music-player";

/** The row under a hero: one large white play button, and shuffle beside it. */
export default function Actions({ tracks, className }: { tracks: Track[]; className?: string }) {
  const player = useMusicPlayer();
  if (tracks.length === 0) return null;
  return (
    <div className={cn("relative flex items-center gap-5 px-4 py-4 md:px-8", className)}>
      <button
        type="button"
        aria-label="Play"
        onClick={() => player.play(tracks, 0)}
        className="flex size-14 cursor-pointer items-center justify-center rounded-full bg-white text-black transition-transform hover:scale-105"
      >
        <Play className="size-6 fill-current" />
      </button>
      <button
        type="button"
        aria-label="Shuffle"
        onClick={() => player.playShuffled(tracks)}
        className="flex size-10 cursor-pointer items-center justify-center rounded-full text-white/60 transition-colors hover:text-white"
      >
        <Shuffle className="size-6" />
      </button>
    </div>
  );
}
