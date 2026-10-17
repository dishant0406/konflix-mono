"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { get } from "@/lib/api";
import { names, streamUrl, type Track } from "@/lib/music";
import { sameItem, type Item } from "@/lib/party/protocol";
import { fix, positionNow } from "@/lib/party/sync";

import { useParty } from "./party";

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
  /** Stops what is playing, where it is, so a video can have the sound to itself. */
  pause: () => void;
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

/** `readyState` at which there is enough sound ahead to be moving. */
const HAVE_FUTURE_DATA = 3;

/** How often a host says where the music is while it plays. */
const PARTY_BEAT = 2000;

/** How often a guest checks it is in step with the host. */
const PARTY_CHECK = 1000;

const GUEST_MESSAGE = "Only the host chooses what plays.";

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

function itemOf(track: Track): Item {
  return { kind: "music", id: track.id, title: track.title, artist: names(track.artists), cover: track.cover };
}

/** What a guest knows of the host's track: enough to show and to ask for the audio. */
function trackOf(item: Extract<Item, { kind: "music" }>): Track {
  return { id: item.id, title: item.title, artists: [{ id: null, name: item.artist }], album: null, duration: null, explicit: false, cover: item.cover ?? null };
}

/** One audio element for the whole site, so the music carries on as pages change. */
export default function MusicPlayerProvider({ children }: { children: React.ReactNode }) {
  const audio = useRef<HTMLAudioElement>(null);
  const party = useParty();
  // The party as the timers and handlers below see it, so they need not be rebuilt each time it changes.
  const partyNow = useRef(party);
  useEffect(() => {
    partyNow.current = party;
  });
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
  // The track whose "what plays next" has been asked for, so each is asked for once.
  const radioFor = useRef<string | null>(null);
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
    partyNow.current.setStatus("loading", current.id);
    element.src = streamUrl(current.id);
    element.play().catch(() => setPlaying(false));
  }, [current?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (audio.current) audio.current.volume = volume;
  }, [volume]);

  // Autoplay: when the last track of the queue starts, what YouTube Music plays after it is added,
  // so the music does not stop. Not while repeating (the queue loops), and not for a guest, whose
  // music is the host's choice.
  useEffect(() => {
    if (!current || index !== queue.length - 1 || repeat !== "off" || party.role === "guest") return undefined;
    if (radioFor.current === current.id) return undefined;
    radioFor.current = current.id;
    get<{ songs: Track[] }>(`/v1/music/radio/${encodeURIComponent(current.id)}`)
      .then(({ songs }) => {
        const add = (list: Track[]) => {
          const known = new Set(list.map((track) => track.id));
          return [...list, ...songs.filter((track) => !known.has(track.id))];
        };
        setQueue(add);
        if (ordered.current) ordered.current = add(ordered.current);
      })
      .catch(() => {
        // Nothing more to play after this one, which is what happened before there was autoplay.
      });
    return undefined;
  }, [current, index, queue.length, repeat, party.role]);

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

  /** A guest cannot choose what plays: that is the host's. */
  const guestBlocked = useCallback(() => {
    if (partyNow.current.role !== "guest") return false;
    toast(GUEST_MESSAGE);
    return true;
  }, []);

  const play = useCallback((tracks: Track[], at = 0) => !guestBlocked() && start(tracks, at, shuffle), [guestBlocked, start, shuffle]);
  const playShuffled = useCallback(
    (tracks: Track[]) => !guestBlocked() && start(tracks, Math.floor(Math.random() * tracks.length), true),
    [guestBlocked, start],
  );

  const next = useCallback(() => {
    if (guestBlocked()) return;
    if (index + 1 < queue.length) setIndex(index + 1);
    else if (repeat === "all" && queue.length > 0) setIndex(0);
    else audio.current?.pause();
  }, [guestBlocked, index, queue.length, repeat]);

  const previous = useCallback(() => {
    if (guestBlocked()) return;
    const element = audio.current;
    if (element && (element.currentTime > RESTART_AFTER || index === 0)) element.currentTime = 0;
    else setIndex((i) => Math.max(0, i - 1));
  }, [guestBlocked, index]);

  const toggle = useCallback(() => {
    const element = audio.current;
    if (!element || !current) return;
    // A guest asks the host, and plays or pauses when the host says so.
    if (partyNow.current.role === "guest") return partyNow.current.ask({ t: element.paused ? "play" : "pause" });
    if (element.paused) void element.play();
    else element.pause();
  }, [current]);

  const pause = useCallback(() => audio.current?.pause(), []);

  const seek = useCallback((seconds: number) => {
    if (partyNow.current.role === "guest") return partyNow.current.ask({ t: "seek", position: seconds });
    if (audio.current) audio.current.currentTime = seconds;
  }, []);

  const jump = useCallback(
    (i: number) => {
      if (guestBlocked()) return;
      if (i === index) {
        if (audio.current) audio.current.currentTime = 0;
      } else setIndex(i);
    },
    [guestBlocked, index],
  );

  const toggleShuffle = useCallback(() => {
    if (guestBlocked()) return;
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
  }, [current, guestBlocked, queue, index, shuffle]);

  const cycleRepeat = useCallback(() => {
    if (!guestBlocked()) setRepeat((r) => (r === "off" ? "all" : r === "all" ? "one" : "off"));
  }, [guestBlocked]);

  // Hosting: the party is told what is playing when it changes, and every couple of seconds.
  const publish = useCallback(
    (heartbeat: boolean) => {
      const element = audio.current;
      if (partyNow.current.role !== "host" || !element || !current) return;
      const item = itemOf(current);
      // A track that is not playing and is not what the party is playing has nothing to say: it
      // must not take the party back from the film that is.
      if (element.paused && !sameItem(partyNow.current.item, item)) return;
      // Playing means the sound is moving: a host that is still loading or buffering is not.
      const moving = !element.paused && element.readyState >= HAVE_FUTURE_DATA;
      partyNow.current.publish({ item, playing: moving, position: element.currentTime }, heartbeat);
    },
    [current],
  );
  useEffect(() => publish(false), [publish, playing, party.role]);
  useEffect(() => {
    if (!playing || party.role !== "host") return undefined;
    const timer = setInterval(() => publish(true), PARTY_BEAT);
    return () => clearInterval(timer);
  }, [playing, party.role, publish]);

  // Hosting: what guests ask for. Only while the music is what the party is playing.
  useEffect(
    () =>
      party.onRequest((request) => {
        const element = audio.current;
        if (!element || partyNow.current.item?.kind !== "music") return;
        if (request.t === "play") void element.play();
        else if (request.t === "pause") element.pause();
        else element.currentTime = Math.min(request.position, element.duration || request.position);
      }),
    [party.onRequest], // eslint-disable-line react-hooks/exhaustive-deps
  );

  // Following: play what the host plays, where the host is.
  useEffect(() => {
    if (party.role !== "guest") return undefined;
    const element = audio.current;
    const follow = () => {
      const target = partyNow.current.target;
      if (!element || !target) return;
      const item = target.playback.item;
      if (item?.kind !== "music") {
        // The party is playing something else, which has its own player.
        if (!element.paused) element.pause();
        return;
      }
      if (element.dataset.track !== item.id) {
        start([trackOf(item)], 0, false);
        return;
      }
      // Until the track has loaded there is no position to correct.
      if (element.readyState < 1) return;
      const change = fix(element.currentTime, positionNow(target), target.playback.playing);
      if (change.seek !== null) element.currentTime = change.seek;
      element.playbackRate = change.speed;
      if (target.playback.playing && element.paused) element.play().catch(() => {});
      else if (!target.playback.playing && !element.paused) element.pause();
    };
    follow();
    const timer = setInterval(follow, PARTY_CHECK);
    return () => {
      clearInterval(timer);
      if (element) element.playbackRate = 1;
    };
  }, [party.role, party.target, start]);

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
    partyNow.current.setStatus("trouble", current.id);
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
      pause,
      next,
      previous,
      seek,
      setVolume: setVolumeState,
      toggleShuffle,
      cycleRepeat,
      jump,
      active: current !== null,
    }),
    [queue, index, current, playing, loading, time, duration, volume, shuffle, repeat, play, playShuffled, toggle, pause, next, previous, seek, toggleShuffle, cycleRepeat, jump],
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
        onPlaying={() => {
          setLoading(false);
          publish(false);
        }}
        onCanPlay={() => current && partyNow.current.setStatus("ready", current.id)}
        onSeeked={() => publish(false)}
        onWaiting={() => {
          setLoading(true);
          if (current) partyNow.current.setStatus("loading", current.id);
          publish(false);
        }}
        onTimeUpdate={(event) => setTime(event.currentTarget.currentTime)}
        onDurationChange={(event) => setDuration(event.currentTarget.duration)}
        onEnded={onEnded}
        onError={onError}
      />
    </Context.Provider>
  );
}
