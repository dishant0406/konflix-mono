"use client";

import { Play, Shuffle } from "lucide-react";

import type { Track } from "@/lib/music";
import { cn } from "@/lib/utils";

import Bone from "./bone";
import { useMusicPlayer } from "@/providers/music-player";

/** The row under a hero: one large white play button, and shuffle beside it. */
export default function Actions({ tracks, className }: { tracks: Track[]; className?: string }) {
  const player = useMusicPlayer();
  if (tracks.length === 0) return null;
  return (
    <div className={cn("relative flex items-center gap-5 gutter py-4", className)}>
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

/** The play and shuffle buttons, as grey circles of the same size. */
export function ActionsSkeleton() {
  return (
    <div className="relative flex items-center gap-5 gutter py-4" aria-hidden>
      <Bone className="size-14 rounded-full" />
      <Bone className="size-10 rounded-full" />
    </div>
  );
}
