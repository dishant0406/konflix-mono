"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { names, streamUrl, type Track } from "@/lib/music";

export type Repeat = "off" | "all" | "one";

interface Player {
  queue: Track[];
  index: number;
  current: Track | null;
  playing: boolean;
  /** True from pressing play until the first sound, when the server is still finding the audio. */
  loading: boolean;
  time: number;
  duration: number;
  volume: number;
  shuffle: boolean;
  repeat: Repeat;
  /** Plays `tracks` from `start`; the rest of them are what Next goes through. */
  play: (tracks: Track[], start?: number) => void;
  /** Plays `tracks` in a random order. */
  playShuffled: (tracks: Track[]) => void;
  toggle: () => void;
  next: () => void;
  previous: () => void;
  seek: (seconds: number) => void;
  setVolume: (volume: number) => void;
  toggleShuffle: () => void;
  cycleRepeat: () => void;
  /** Jumps to a place in the queue. */
  jump: (index: number) => void;
  /** Whether a bar is showing, so the page can leave room for it. */
  active: boolean;
}

const Context = createContext<Player | null>(null);

export function useMusicPlayer(): Player {
  const player = useContext(Context);
  if (!player) throw new Error("useMusicPlayer needs MusicPlayerProvider");
  return player;
}

/** Pressing Previous this far into a track starts it again instead of going back. */
const RESTART_AFTER = 3;

function shuffled<T>(list: T[]): T[] {
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** One audio element for the whole site, so the music carries on as pages change. */
export default function MusicPlayerProvider({ children }: { children: React.ReactNode }) {
  const audio = useRef<HTMLAudioElement>(null);
  const [queue, setQueue] = useState<Track[]>([]);
  const [index, setIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [loading, setLoading] = useState(false);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(1);
  const [shuffle, setShuffle] = useState(false);
  const [repeat, setRepeat] = useState<Repeat>("off");
  // The queue as it was before shuffling, so turning shuffle off puts it back.
  const ordered = useRef<Track[] | null>(null);
  // The track whose audio was asked for again after failing, so one bad address is retried once.
  const retried = useRef<string | null>(null);

  const current = queue[index] ?? null;

  // A new track: point the element at it.
  useEffect(() => {
    const element = audio.current;
    if (!element || !current) return;
    setTime(0);
    setDuration(0);
    setLoading(true);
    element.src = streamUrl(current.id);
    element.play().catch(() => setPlaying(false));
  }, [current?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (audio.current) audio.current.volume = volume;
  }, [volume]);

  const start = useCallback((tracks: Track[], at: number, shuffleOn: boolean) => {
    ordered.current = shuffleOn ? tracks : null;
    const first = tracks[at];
    const list = shuffleOn && first ? [first, ...shuffled(tracks.filter((_, i) => i !== at))] : tracks;
    retried.current = null;
    setShuffle(shuffleOn);
    setQueue(list);
    setIndex(shuffleOn ? 0 : at);
    // The same track pressed again restarts: the effect above only runs for a different one.
    if (first && first.id === audio.current?.dataset.track) {
      audio.current.currentTime = 0;
      void audio.current.play();
    }
  }, []);

  const play = useCallback((tracks: Track[], at = 0) => start(tracks, at, shuffle), [start, shuffle]);
  const playShuffled = useCallback(
    (tracks: Track[]) => start(tracks, Math.floor(Math.random() * tracks.length), true),
    [start],
  );

  const next = useCallback(() => {
    if (index + 1 < queue.length) setIndex(index + 1);
    else if (repeat === "all" && queue.length > 0) setIndex(0);
    else audio.current?.pause();
  }, [index, queue.length, repeat]);

  const previous = useCallback(() => {
    const element = audio.current;
    if (element && (element.currentTime > RESTART_AFTER || index === 0)) element.currentTime = 0;
    else setIndex((i) => Math.max(0, i - 1));
  }, [index]);

  const toggle = useCallback(() => {
    const element = audio.current;
    if (!element || !current) return;
    if (element.paused) void element.play();
    else element.pause();
  }, [current]);

  const seek = useCallback((seconds: number) => {
    if (audio.current) audio.current.currentTime = seconds;
  }, []);

  const jump = useCallback(
    (i: number) => {
      if (i === index) {
        if (audio.current) audio.current.currentTime = 0;
      } else setIndex(i);
    },
    [index],
  );

  const toggleShuffle = useCallback(() => {
    if (!current) return setShuffle((on) => !on);
    if (!shuffle) {
      ordered.current = queue;
      setQueue([...queue.slice(0, index + 1), ...shuffled(queue.slice(index + 1))]);
      setShuffle(true);
    } else {
      const back = ordered.current ?? queue;
      setQueue(back);
      setIndex(Math.max(0, back.findIndex((t) => t.id === current.id)));
      ordered.current = null;
      setShuffle(false);
    }
  }, [current, queue, index, shuffle]);

  const cycleRepeat = useCallback(() => setRepeat((r) => (r === "off" ? "all" : r === "all" ? "one" : "off")), []);

  // The phone's lock screen and the keyboard's media keys.
  useEffect(() => {
    if (!current || !("mediaSession" in navigator)) return;
    navigator.mediaSession.metadata = new MediaMetadata({
      title: current.title,
      artist: names(current.artists),
      album: current.album?.name ?? "",
      artwork: current.cover ? [{ src: current.cover }] : [],
    });
    navigator.mediaSession.setActionHandler("play", () => void audio.current?.play());
    navigator.mediaSession.setActionHandler("pause", () => audio.current?.pause());
    navigator.mediaSession.setActionHandler("previoustrack", previous);
    navigator.mediaSession.setActionHandler("nexttrack", next);
    navigator.mediaSession.setActionHandler("seekto", (d) => d.seekTime != null && seek(d.seekTime));
  }, [current, previous, next, seek]);

  const onError = () => {
    if (!current) return;
    const element = audio.current;
    // The address may have expired: ask once more, which makes the server look it up again.
    if (retried.current !== current.id && element) {
      retried.current = current.id;
      const at = element.currentTime;
      element.src = streamUrl(current.id);
      element.currentTime = at;
      element.play().catch(() => setPlaying(false));
      return;
    }
    setLoading(false);
    setPlaying(false);
    toast.error(`Could not play "${current.title}".`);
    if (index + 1 < queue.length) setIndex(index + 1);
  };

  const onEnded = () => {
    if (repeat === "one" && audio.current) {
      audio.current.currentTime = 0;
      void audio.current.play();
    } else if (index + 1 < queue.length) setIndex(index + 1);
    else if (repeat === "all" && queue.length > 0) setIndex(0);
    else setPlaying(false);
  };

  const value = useMemo<Player>(
    () => ({
      queue,
      index,
      current,
      playing,
      loading,
      time,
      duration,
      volume,
      shuffle,
      repeat,
      play,
      playShuffled,
      toggle,
      next,
      previous,
      seek,
      setVolume: setVolumeState,
      toggleShuffle,
      cycleRepeat,
      jump,
      active: current !== null,
    }),
    [queue, index, current, playing, loading, time, duration, volume, shuffle, repeat, play, playShuffled, toggle, next, previous, seek, toggleShuffle, cycleRepeat, jump],
  );

  return (
    <Context.Provider value={value}>
      {children}
      <audio
        ref={audio}
        data-track={current?.id}
        preload="auto"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onPlaying={() => setLoading(false)}
        onWaiting={() => setLoading(true)}
        onTimeUpdate={(event) => setTime(event.currentTarget.currentTime)}
        onDurationChange={(event) => setDuration(event.currentTarget.duration)}
        onEnded={onEnded}
        onError={onError}
      />
    </Context.Provider>
  );
}
