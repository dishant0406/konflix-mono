"use client";

import { ChevronDown, ListMusic, Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward, Volume2, VolumeX, X } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import { Spinner } from "@/components/states";
import { albumPath, clock, names } from "@/lib/music";
import { cn } from "@/lib/utils";
import { useMusicPlayer } from "@/providers/music-player";

import Cover from "./cover";
import Equaliser from "./equaliser";
import Slider from "./slider";

function Round({ label, on, onClick, disabled, children, className }: { label: string; on?: boolean; onClick: () => void; disabled?: boolean; children: React.ReactNode; className?: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={on}
      disabled={disabled}
      onClick={onClick}
      className={cn("flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-full text-white/60 transition-colors hover:text-white disabled:opacity-30", on && "text-white", className)}
    >
      {children}
    </button>
  );
}

function PlayButton({ big }: { big?: boolean }) {
  const p = useMusicPlayer();
  return (
    <button
      type="button"
      aria-label={p.playing ? "Pause" : "Play"}
      onClick={p.toggle}
      className={cn("flex shrink-0 cursor-pointer items-center justify-center rounded-full bg-white text-black transition-transform hover:scale-105", big ? "size-16" : "size-9")}
    >
      {p.loading ? (
        <Spinner className="border-black/20 border-t-black" />
      ) : p.playing ? (
        <Pause className={cn("fill-current", big ? "size-7" : "size-4")} />
      ) : (
        <Play className={cn("fill-current", big ? "size-7" : "size-4")} />
      )}
    </button>
  );
}

function Transport({ big }: { big?: boolean }) {
  const p = useMusicPlayer();
  return (
    <div className={cn("flex items-center", big ? "gap-6" : "gap-2")}>
      <Round label="Shuffle" on={p.shuffle} onClick={p.toggleShuffle} className="hidden sm:flex">
        <Shuffle className="size-4" />
      </Round>
      <Round label="Previous" onClick={p.previous}>
        <SkipBack className={cn("fill-current", big ? "size-6" : "size-4")} />
      </Round>
      <PlayButton big={big} />
      <Round label="Next" onClick={p.next}>
        <SkipForward className={cn("fill-current", big ? "size-6" : "size-4")} />
      </Round>
      <Round label={`Repeat ${p.repeat}`} on={p.repeat !== "off"} onClick={p.cycleRepeat} className="hidden sm:flex">
        {p.repeat === "one" ? <Repeat1 className="size-4" /> : <Repeat className="size-4" />}
      </Round>
    </div>
  );
}

function Seek({ className }: { className?: string }) {
  const p = useMusicPlayer();
  const total = Number.isFinite(p.duration) && p.duration > 0 ? p.duration : 0;
  return (
    <div className={cn("flex items-center gap-2 text-xs tabular-nums text-white/60", className)}>
      <span className="w-10 text-right">{clock(p.time)}</span>
      <Slider label="Seek" value={p.time} max={total} onChange={p.seek} />
      <span className="w-10">{clock(total)}</span>
    </div>
  );
}

function Queue({ onClose }: { onClose: () => void }) {
  const p = useMusicPlayer();
  const upcoming = p.queue.slice(p.index + 1);
  return (
    <div className="fixed bottom-[80px] right-2 top-16 z-[55] flex w-[min(24rem,calc(100vw-1rem))] flex-col rounded-lg border border-white/[0.08] bg-mpanel md:bottom-[88px]">
      <div className="flex h-14 shrink-0 items-center justify-between border-b border-white/[0.08] px-4">
        <h2 className="text-base font-bold">Queue</h2>
        <Round label="Close queue" onClick={onClose}>
          <X className="size-5" />
        </Round>
      </div>
      <div className="flex-1 overflow-y-auto p-2">
        {p.current && (
          <>
            <p className="px-2 pb-1 pt-2 text-xs font-semibold text-white/50">Now playing</p>
            <QueueRow index={p.index} />
          </>
        )}
        {upcoming.length > 0 && <p className="px-2 pb-1 pt-4 text-xs font-semibold text-white/50">Next up</p>}
        {upcoming.map((_, n) => (
          <QueueRow key={p.index + 1 + n} index={p.index + 1 + n} />
        ))}
      </div>
    </div>
  );
}

function QueueRow({ index }: { index: number }) {
  const p = useMusicPlayer();
  const track = p.queue[index];
  const here = index === p.index;
  return (
    <button
      type="button"
      onClick={() => p.jump(index)}
      className="flex w-full cursor-pointer items-center gap-3 rounded-md p-2 text-left transition-colors hover:bg-white/[0.07]"
    >
      <Cover src={track.cover} sizes="40px" className="size-10 shrink-0 rounded" />
      <span className="min-w-0 flex-1">
        <span className={cn("block truncate text-sm font-medium", here ? "text-white" : "text-white/90")}>{track.title}</span>
        <span className="block truncate text-[13px] text-white/60">{names(track.artists)}</span>
      </span>
      {here && <Equaliser paused={!p.playing} />}
    </button>
  );
}

/** The whole player on a phone: a large cover and room for the controls. */
function NowPlaying({ onClose }: { onClose: () => void }) {
  const p = useMusicPlayer();
  const track = p.current!;
  return (
    <div className="fixed inset-0 z-[70] flex flex-col bg-mbg px-6 pb-8 pt-4 md:hidden">
      <div className="flex h-10 items-center justify-between">
        <Round label="Close" onClick={onClose}>
          <ChevronDown className="size-6" />
        </Round>
        <span className="text-xs font-semibold text-white/60">Now playing</span>
        <span className="size-8" />
      </div>
      <div className="flex flex-1 items-center justify-center py-4">
        <Cover src={track.cover} sizes="400px" priority className="w-full max-w-sm" />
      </div>
      <div className="mb-4 min-w-0">
        <p className="truncate text-2xl font-bold tracking-tight">{track.title}</p>
        <p className="truncate text-base text-white/60">{names(track.artists)}</p>
      </div>
      <Seek className="mb-4" />
      <div className="flex justify-center">
        <Transport big />
      </div>
    </div>
  );
}

/** The bar along the bottom while there is something to play. It outlives page changes. */
export default function PlayerBar() {
  const p = useMusicPlayer();
  const [queue, setQueue] = useState(false);
  const [open, setOpen] = useState(false);
  if (!p.current) return null;
  const track = p.current;
  const total = Number.isFinite(p.duration) && p.duration > 0 ? p.duration : 0;

  return (
    <>
      {/* Room at the end of every page, so the bar never covers its last line. */}
      <div aria-hidden className="h-[72px]" />
      {queue && <Queue onClose={() => setQueue(false)} />}
      {open && <NowPlaying onClose={() => setOpen(false)} />}
      <div className={"fixed inset-x-0 bottom-0 z-50 border-t border-white/[0.08] bg-black text-white"}>
        {/* On a phone the bar is a mini-player: tap it for the full one, with the progress as a line on top. */}
        <div className="absolute inset-x-0 top-0 h-0.5 bg-white/20 md:hidden">
          <div className="h-full bg-white" style={{ width: total ? `${(p.time / total) * 100}%` : 0 }} />
        </div>
        <div className="flex h-[72px] items-center gap-3 px-3 md:px-4">
          <div className="flex min-w-0 flex-1 items-center gap-3 md:w-[30%] md:flex-none">
            <button type="button" onClick={() => setOpen(true)} aria-label="Open player" className="shrink-0 cursor-pointer md:pointer-events-none">
              <Cover src={track.cover} sizes="56px" className="size-14 rounded" />
            </button>
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{track.title}</p>
              {track.album?.id ? (
                <Link href={albumPath(track.album.id)} className="block truncate text-[13px] text-white/60 hover:text-white hover:underline">
                  {names(track.artists)}
                </Link>
              ) : (
                <p className="truncate text-[13px] text-white/60">{names(track.artists)}</p>
              )}
            </div>
          </div>

          <div className="flex shrink-0 flex-col items-center gap-1 md:flex-1">
            <div className="hidden md:block">
              <Transport />
            </div>
            <div className="md:hidden">
              <PlayButton />
            </div>
            <Seek className="hidden w-full max-w-xl md:flex" />
          </div>

          <div className="hidden w-[30%] items-center justify-end gap-1 md:flex">
            <Round label="Queue" on={queue} onClick={() => setQueue((on) => !on)}>
              <ListMusic className="size-[18px]" />
            </Round>
            <Round label={p.volume === 0 ? "Unmute" : "Mute"} onClick={() => p.setVolume(p.volume === 0 ? 1 : 0)}>
              {p.volume === 0 ? <VolumeX className="size-[18px]" /> : <Volume2 className="size-[18px]" />}
            </Round>
            <Slider label="Volume" value={p.volume} max={1} step={0.02} onChange={p.setVolume} className="w-24" />
          </div>
        </div>
      </div>
    </>
  );
}
