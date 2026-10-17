"use client";

import {
  Captions,
  ChevronLeft,
  Layers,
  List,
  Maximize,
  Minimize,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  SkipForward,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";

import { fix, positionNow } from "@/lib/party/sync";
import { DAILY_LIMIT_MESSAGE } from "@/lib/play";
import { optionForChoice, preferRelease, readChoice, shiftCues, writeChoice } from "@/lib/subtitles";
import { cn, formatSeconds } from "@/lib/utils";
import { useMusicPlayer } from "@/providers/music-player";
import { useParty } from "@/providers/party";
import type { SubtitleOption } from "@/types/media";

import { Button, IconButton } from "./button";
import CaptionsMenu from "./captions-menu";
import PartyButton from "./party";
import { SidePanel } from "./side-panel";
import { Spinner } from "./states";

/** "Next episode" is offered this many seconds before the end. */
const NEXT_OFFER = 40;
/** How long the controls stay after the last movement. */
const IDLE_FOR = 3000;
/** How often the position is reported. */
const REPORT_EVERY = 5000;
/** A host waits at least this long, and at most the longer one, for guests that are still loading. */
const HOLD_AT_LEAST = 1000;
const HOLD_AT_MOST = 45_000;
/** `readyState` at which there is enough picture ahead to be moving. */
const HAVE_FUTURE_DATA = 3;
/** How often a host says where the picture is while it plays, and how often a guest checks it is in step. */
const PARTY_BEAT = 2000;
const PARTY_CHECK = 1000;
/** How far the captions rise to stay clear of the control bar while it is showing. */
const CUES_LIFT = "5.5rem";
type Panel = "episodes" | "sources" | "subtitles";
const NO_SUBTITLES: SubtitleOption[] = [];

// A live HLS playlist has no total length, so the element reports Infinity. That is not a
// duration, and using it makes the seek bar's maths invalid.
function finite(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

type Props = {
  /** The media id of what is playing, which a watch party names it by. */
  mediaId: string;
  /** The playable URL returned by the API. */
  src: string;
  /** "hls" when the API had to convert the container, "file" when the browser plays it directly. */
  streamType: string | null;
  /** The subtitle files the API found; empty when there are none. */
  subtitles?: SubtitleOption[];
  /** The name of the release being played, so the subtitles made for it are offered first. */
  release?: string | null;
  /** Still looking for them, so the captions button says so instead of "none". */
  subtitlesLoading?: boolean;
  /** The line in the top bar: title, episode, name. */
  title: string;
  /** Where to start, in seconds, when the viewer left part way. */
  startAt: number | null;
  onBack: () => void;
  onError: (message: string) => void;
  /** Called once, the first time the picture is actually playing. */
  onStarted?: () => void;
  /** Reported every few seconds and when playback pauses, so the home page can offer to carry on. */
  onProgress: (position: number, duration: number) => void;
  /** Absent for a film or the last episode. */
  onNext?: () => void;
  /** The episode list, shown in a side panel from the list button. Absent for a film. */
  episodes?: ReactNode;
  /**
   * The source list, shown in a side panel from the sources button. It is given where playback is
   * now (so a different release can resume there) and a way to close the panel.
   */
  sources?: (controls: { position: () => number; close: () => void }) => ReactNode;
};

/**
 * The picture and its controls, over the whole window: a top bar with the way back, a bottom
 * bar with the transport, and an episode panel at the side. Everything but the picture fades
 * out while the pointer is still.
 *
 * Two source kinds need genuinely different handling, decided from what the API reported:
 *   hls   the container is not one a browser can play (an MKV release), so the API serves an
 *         HLS playlist. Chrome and Firefox need hls.js; Safari plays it natively.
 *   file  bytes the browser plays on its own, set as the element's src.
 */
export default function Player({
  mediaId,
  src,
  streamType,
  subtitles: listed = NO_SUBTITLES,
  release = null,
  subtitlesLoading = false,
  title,
  startAt,
  onBack,
  onError,
  onStarted,
  onProgress,
  onNext,
  episodes,
  sources,
}: Props) {
  const video = useRef<HTMLVideoElement>(null);
  const frame = useRef<HTMLDivElement>(null);
  const idle = useRef<ReturnType<typeof setTimeout> | null>(null);
  const resumed = useRef(false);
  /** Where to jump to once the file (or, for HLS, the conversion) has reached it. */
  const pendingStart = useRef<number | null>(null);

  const subtitles = useMemo(() => preferRelease(listed, release), [listed, release]);
  /** How far the viewer has moved the subtitles, in seconds, and how far the shown cues already are. */
  const [subtitleShift, setSubtitleShift] = useState(0);
  const shiftNow = useRef(0);
  const shiftApplied = useRef(0);
  const shiftedTrack = useRef<string | null>(null);

  const [ready, setReady] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [visible, setVisible] = useState(true);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  /** When a host began waiting for guests that are still loading, or null when it is not waiting. */
  const [heldSince, setHeldSince] = useState<number | null>(null);
  /** The subtitle file showing; null is off. */
  const [active, setActive] = useState<SubtitleOption | null>(null);
  /** Set once the viewer's remembered language has been looked at, or they have chosen themselves. */
  const decided = useRef(false);

  const isHls = streamType === "hls";

  // The party as the timers and handlers below see it, so they need not be rebuilt each time it changes.
  const music = useMusicPlayer();
  const party = useParty();
  const partyNow = useRef(party);
  useEffect(() => {
    partyNow.current = party;
  });

  // Callbacks are held in refs so a caller passing inline functions cannot tear down and
  // rebuild the stream on every render.
  const callbacks = useRef({ onError, onStarted, onProgress, onNext });
  const started = useRef(false);
  useEffect(() => {
    callbacks.current = { onError, onStarted, onProgress, onNext };
  });

  useEffect(() => {
    const element = video.current;
    if (!element || !src) return undefined;
    let cancelled = false;
    resumed.current = false;
    pendingStart.current = startAt;
    setReady(false);

    // A conversion is made as the viewer watches, so a resume deep into the file has to wait until
    // it has got there. The server is told where the viewer means to start, so it runs that far.
    const seekWhenReachable = setInterval(() => {
      const wanted = pendingStart.current;
      if (wanted === null) return;
      const ranges = element.seekable;
      const reachable = ranges.length > 0 ? ranges.end(ranges.length - 1) : 0;
      if (reachable > wanted + 1) {
        element.currentTime = wanted;
        pendingStart.current = null;
      }
    }, 500);

    const plain = () => {
      element.src = src;
      return () => {
        element.removeAttribute("src");
        element.load();
      };
    };

    // hls.js is loaded only for a stream that needs it. A file a browser plays as it is, which is
    // most films, never downloads its 150KB.
    let stop: (() => void) | null = null;
    const attach = async () => {
      if (!isHls) {
        stop = plain();
        return;
      }
      const { default: Hls } = await import("hls.js");
      if (cancelled) return;
      // Safari plays HLS natively, and on iOS MediaSource is unavailable.
      if (!Hls.isSupported()) {
        stop = plain();
        return;
      }

      // A playlist that is still growing counts as live, and a live player starts near its end.
      // This one starts at the beginning, whatever the conversion has got to.
      const hls = new Hls({ enableWorker: true, maxBufferLength: 30, startPosition: 0 });
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        if (!cancelled) setReady(true);
      });
      hls.on(Hls.Events.ERROR, (_event, data) => {
        // Only a fatal error means playback has stopped; hls.js recovers from the rest.
        if (!data.fatal) return;
        // A manifest that will not load cannot be fixed by loading it again: the usual cause
        // is an expired session, which answers 404 every time.
        if (data.response?.code === 429) {
          // Loading again would only be refused again, until tomorrow.
          hls.destroy();
          callbacks.current.onError(DAILY_LIMIT_MESSAGE);
        } else if (data.details === Hls.ErrorDetails.MANIFEST_LOAD_ERROR) {
          hls.destroy();
          callbacks.current.onError("This stream has expired. Reload the episode to start it again.");
        } else if (data.type === Hls.ErrorTypes.NETWORK_ERROR) hls.startLoad();
        else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) hls.recoverMediaError();
        else {
          hls.destroy();
          callbacks.current.onError("Playback failed. Reload the episode to try again.");
        }
      });
      hls.loadSource(src);
      hls.attachMedia(element);
      stop = () => hls.destroy();
    };
    void attach();

    return () => {
      cancelled = true;
      clearInterval(seekWhenReachable);
      stop?.();
    };
  }, [src, isHls, startAt]);

  const say = useCallback((text: string) => {
    setMessage(text);
    window.setTimeout(() => setMessage((current) => (current === text ? null : current)), 1400);
  }, []);

  const choose = useCallback(
    (option: SubtitleOption | null) => {
      decided.current = true;
      setActive(option);
      writeChoice(option?.lang ?? "off");
      say(option ? `Subtitles: ${option.language}` : "Subtitles off");
    },
    [say],
  );

  // The list can arrive after the picture has started. The language the viewer last chose is
  // switched on once, when it is offered here; after that only they change it.
  useEffect(() => {
    if (decided.current || subtitles.length === 0) return;
    decided.current = true;
    const option = optionForChoice(subtitles, readChoice());
    if (option) {
      setActive(option);
      say(`Subtitles: ${option.language}`);
    }
  }, [subtitles, say]);

  // A <track> added after the element exists is not in `textTracks` until the next tick, and a
  // track nobody switched on stays hidden, so the one chosen is found by its id and shown.
  useEffect(() => {
    const element = video.current;
    if (!element) return undefined;
    const apply = () => {
      for (let index = 0; index < element.textTracks.length; index += 1) {
        const track = element.textTracks[index];
        track.mode = active && track.id === active.id ? "showing" : "disabled";
      }
    };
    const timer = window.setTimeout(apply, 0);
    element.textTracks.addEventListener("addtrack", apply);
    return () => {
      window.clearTimeout(timer);
      element.textTracks.removeEventListener("addtrack", apply);
    };
  }, [active]);

  // A file made for another cut of the film runs early or late; z and x move it a tenth of a
  // second. The cues are moved where they are, so it needs no new file, and a track loaded later
  // starts from its own times.
  const shiftCuesNow = useCallback(() => {
    const track = active ? Array.from(video.current?.textTracks ?? []).find((t) => t.id === active.id) : undefined;
    if (!track?.cues) return;
    if (shiftedTrack.current !== track.id) {
      shiftedTrack.current = track.id;
      shiftApplied.current = 0;
    }
    shiftCues(Array.from(track.cues), subtitleShift - shiftApplied.current);
    shiftApplied.current = subtitleShift;
  }, [active, subtitleShift]);
  useEffect(shiftCuesNow, [shiftCuesNow]);

  const shiftSubtitles = useCallback(
    (delta: number) => {
      const next = Math.round((shiftNow.current + delta) * 10) / 10;
      shiftNow.current = next;
      setSubtitleShift(next);
      say(next === 0 ? "Subtitle timing: in step" : `Subtitle timing: ${next > 0 ? "+" : ""}${next.toFixed(1)}s`);
    },
    [say],
  );

  const toggle = useCallback(() => {
    const element = video.current;
    if (!element) return;
    // A guest asks the host, and plays or pauses when the host says so.
    if (partyNow.current.role === "guest") return partyNow.current.ask({ t: element.paused ? "play" : "pause" });
    if (element.paused) element.play().catch(() => callbacks.current.onError("Playback could not be started."));
    else element.pause();
  }, []);

  const seekBy = useCallback(
    (delta: number) => {
      const element = video.current;
      if (!element) return;
      const to = Math.min(Math.max(0, element.currentTime + delta), element.duration || 0);
      if (partyNow.current.role === "guest") partyNow.current.ask({ t: "seek", position: to });
      else element.currentTime = to;
      say(`${delta > 0 ? "+" : "−"}${Math.abs(delta)}s`);
    },
    [say],
  );

  const seekTo = useCallback((to: number) => {
    if (partyNow.current.role === "guest") partyNow.current.ask({ t: "seek", position: to });
    else if (video.current) video.current.currentTime = to;
  }, []);

  // Hosting: the party is told what is playing when it changes, and every couple of seconds.
  const publish = useCallback(
    (heartbeat: boolean) => {
      const element = video.current;
      if (partyNow.current.role !== "host" || !element) return;
      // Playing means the picture is moving: a host that is still loading or buffering is not.
      const moving = !element.paused && element.readyState >= HAVE_FUTURE_DATA;
      partyNow.current.publish({ item: { kind: "video", id: mediaId }, playing: moving, position: element.currentTime }, heartbeat);
    },
    [mediaId],
  );
  useEffect(() => publish(false), [publish, party.role]);
  useEffect(() => {
    if (!playing || party.role !== "host") return undefined;
    const timer = setInterval(() => publish(true), PARTY_BEAT);
    return () => clearInterval(timer);
  }, [playing, party.role, publish]);

  // Hosting: starts when the guests are ready, or after a while whether they are or not.
  const { role, waitingFor } = party;
  useEffect(() => {
    if (heldSince === null) return undefined;
    const release = () => {
      setHeldSince(null);
      video.current?.play().catch(() => {});
    };
    const wait = role !== "host" ? 0 : Math.max(0, (waitingFor.length === 0 ? heldSince + HOLD_AT_LEAST : heldSince + HOLD_AT_MOST) - Date.now());
    const timer = setTimeout(release, wait);
    return () => clearTimeout(timer);
  }, [heldSince, role, waitingFor]);

  // Hosting: what guests ask for, while this is what the party is playing.
  useEffect(
    () =>
      party.onRequest((request) => {
        const element = video.current;
        const item = partyNow.current.item;
        if (!element || item?.kind !== "video" || item.id !== mediaId) return;
        if (request.t === "play") element.play().catch(() => {});
        else if (request.t === "pause") element.pause();
        else element.currentTime = Math.min(request.position, element.duration || request.position);
      }),
    [party.onRequest, mediaId], // eslint-disable-line react-hooks/exhaustive-deps
  );

  // A host who leaves the film tells the party nothing is playing, so guests do not carry on without it.
  useEffect(
    () => () => {
      const now = partyNow.current;
      if (now.role === "host" && now.item?.kind === "video" && now.item.id === mediaId) {
        now.publish({ item: null, playing: false, position: 0 });
      }
    },
    [mediaId],
  );

  // Following: be where the host is, playing or paused, a little faster or slower to close a small gap.
  useEffect(() => {
    if (party.role !== "guest") return undefined;
    const element = video.current;
    const follow = () => {
      const target = partyNow.current.target;
      // Until the picture has loaded there is no position to correct.
      if (!element || !target || element.readyState < 1) return;
      const item = target.playback.item;
      // The host has stopped.
      if (!item) {
        if (!element.paused) element.pause();
        return;
      }
      // What else the party is playing is another page's to show.
      if (item.kind !== "video" || item.id !== mediaId) return;
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
  }, [party.role, party.target, mediaId]);

  const toggleFullscreen = useCallback(async () => {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await frame.current?.requestFullscreen();
    } catch {
      // Refused (iPhone Safari does not allow it on an arbitrary element); silent.
    }
  }, []);

  useEffect(() => {
    if (video.current) video.current.volume = volume;
  }, [volume]);
  useEffect(() => {
    if (video.current) video.current.muted = muted;
  }, [muted]);
  useEffect(() => {
    const change = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener("fullscreenchange", change);
    return () => document.removeEventListener("fullscreenchange", change);
  }, []);

  const wake = useCallback(() => {
    setVisible(true);
    if (idle.current) clearTimeout(idle.current);
    // Controls stay while paused, so a paused frame never hides the way to resume.
    idle.current = setTimeout(() => {
      if (!video.current?.paused) setVisible(false);
    }, IDLE_FOR);
  }, []);
  useEffect(() => () => {
    if (idle.current) clearTimeout(idle.current);
  }, []);

  // Report the position on a timer, and once more on the way out.
  useEffect(() => {
    const report = () => {
      const element = video.current;
      if (element && finite(element.duration) > 0) callbacks.current.onProgress(element.currentTime, element.duration);
    };
    const timer = setInterval(report, REPORT_EVERY);
    return () => {
      clearInterval(timer);
      report();
    };
  }, [src]);

  // Keys act anywhere on the page while the player is up, except in a field.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName) || event.metaKey || event.ctrlKey) return;
      const handled = (() => {
        switch (event.key) {
          case " ":
          case "k":
            toggle();
            return true;
          case "ArrowRight":
            seekBy(10);
            return true;
          case "ArrowLeft":
            seekBy(-10);
            return true;
          case "ArrowUp":
            setVolume((value) => Math.min(1, value + 0.1));
            setMuted(false);
            return true;
          case "ArrowDown":
            setVolume((value) => Math.max(0, value - 0.1));
            return true;
          case "m":
            setMuted((value) => !value);
            return true;
          case "z":
            shiftSubtitles(-0.1);
            return true;
          case "x":
            shiftSubtitles(0.1);
            return true;
          case "f":
            void toggleFullscreen();
            return true;
          case "Escape":
            if (panel) setPanel(null);
            return panel !== null;
          default:
            return false;
        }
      })();
      if (handled) {
        event.preventDefault();
        wake();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [panel, seekBy, shiftSubtitles, toggle, toggleFullscreen, wake]);

  const remaining = Math.max(duration - position, 0);
  const offerNext = Boolean(onNext) && duration > 0 && remaining <= NEXT_OFFER;
  const shown = visible || !playing || panel !== null;
  const togglePanel = (which: Panel) => {
    setPanel((open) => (open === which ? null : which));
  };

  return (
    <div
      ref={frame}
      onMouseMove={wake}
      onTouchStart={wake}
      style={{ "--cues-lift": shown ? CUES_LIFT : "0px" } as CSSProperties}
      className={cn("fixed inset-0 z-[60] overflow-hidden bg-black", !shown && "cursor-none")}
    >
      <video
        ref={video}
        playsInline
        crossOrigin="anonymous"
        preload="metadata"
        className="size-full"
        onClick={toggle}
        onDoubleClick={toggleFullscreen}
        onLoadedMetadata={(event) => {
          const element = event.currentTarget;
          setDuration(finite(element.duration));
          setReady(true);
          if (!resumed.current) {
            const now = partyNow.current;
            // A host with company waits for the guests to be ready, so no one starts behind; a
            // guest starts when the host does. Anyone else starts on their own, as they pressed Play
            // to get here, and a browser that refuses leaves the play button for them.
            if (now.role === "host" && now.members.length > 1) setHeldSince(Date.now());
            else if (now.role !== "guest") element.play().catch(() => {});
          }
          resumed.current = true;
        }}
        onCanPlay={() => partyNow.current.setStatus("ready", mediaId)}
        onDurationChange={(event) => setDuration(finite(event.currentTarget.duration))}
        onPlay={() => {
          setPlaying(true);
          wake();
          publish(false);
          // The picture has the sound to itself.
          music.pause();
        }}
        onSeeked={() => publish(false)}
        onPause={() => {
          setPlaying(false);
          setVisible(true);
          publish(false);
          const element = video.current;
          if (element && finite(element.duration) > 0) callbacks.current.onProgress(element.currentTime, element.duration);
        }}
        onTimeUpdate={(event) => {
          const element = event.currentTarget;
          setPosition(element.currentTime);
          if (element.buffered.length > 0) setBuffered(element.buffered.end(element.buffered.length - 1));
        }}
        onWaiting={() => {
          setReady(false);
          partyNow.current.setStatus("loading", mediaId);
          publish(false);
        }}
        onPlaying={() => {
          setReady(true);
          publish(false);
          if (!started.current) {
            started.current = true;
            callbacks.current.onStarted?.();
          }
        }}
        onEnded={() => callbacks.current.onNext?.()}
        onError={(event) => {
          // The common real failure is a codec the browser lacks; "could not play" with no
          // detail is what makes that look like a broken site.
          const code = event.currentTarget.error?.code;
          if (code === 4) {
            callbacks.current.onError("Your browser cannot play this file. The release uses a codec it does not support.");
            return;
          }
          // The element does not say why a request failed, so ask: a HEAD sends no video.
          void fetch(src, { method: "HEAD" })
            .then((response) => response.status === 429)
            .catch(() => false)
            .then((limited) =>
              callbacks.current.onError(
                limited
                  ? DAILY_LIMIT_MESSAGE
                  : "Playback failed. The stream may have expired - reload the episode to try again.",
              ),
            );
        }}
      >
        {active && <track key={active.id} id={active.id} kind="subtitles" src={active.url} srcLang={active.lang} label={active.language} onLoad={shiftCuesNow} />}
      </video>

      {!ready && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2">
          <Spinner />
          <p className="text-sm font-semibold text-ink">{isHls ? "Converting for your browser" : "Buffering"}</p>
        </div>
      )}

      {heldSince !== null && (
        <div className="pointer-events-none absolute left-1/2 top-1/3 -translate-x-1/2 rounded-full bg-black/60 px-4 py-2 text-sm font-semibold text-ink">
          Waiting for {waitingFor.length > 0 ? waitingFor.join(", ") : "everyone"}…
        </div>
      )}

      {message && (
        <div className="pointer-events-none absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-black/60 px-4 py-2 text-sm font-semibold text-ink">
          {message}
        </div>
      )}

      <div className={cn("transition-opacity duration-300", shown ? "opacity-100" : "pointer-events-none opacity-0")}>
        <div className="absolute inset-x-0 top-0 bg-gradient-to-b from-canvas/85 to-transparent">
          <div className="gutter flex h-14 items-center gap-4">
            <IconButton label="Back" onClick={onBack}>
              <ChevronLeft className="size-5" />
            </IconButton>
            <p className="min-w-0 flex-1 truncate text-[15px] font-semibold text-ink">{title}</p>
            <PartyButton />
          </div>
        </div>

        <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-canvas/90 via-canvas/60 to-transparent pt-16">
          <div className="gutter flex flex-col gap-1 pb-4">
            <div className="flex items-center gap-3 text-xs tabular-nums text-ink">
              <span className="w-12 text-right">{formatSeconds(position)}</span>
              <Seek position={position} duration={duration} buffered={buffered} onSeek={seekTo} />
              <span className="w-12">{duration > 0 ? `−${formatSeconds(remaining)}` : "--:--"}</span>
            </div>

            <div className="flex items-center gap-1">
              <IconButton label={playing ? "Pause" : "Play"} onClick={toggle}>
                {playing ? <Pause className="size-5 fill-current" /> : <Play className="size-5 fill-current" />}
              </IconButton>
              <IconButton label="Back 10 seconds" onClick={() => seekBy(-10)}>
                <RotateCcw className="size-[18px]" />
              </IconButton>
              <IconButton label="Forward 10 seconds" onClick={() => seekBy(10)}>
                <RotateCw className="size-[18px]" />
              </IconButton>
              <div className="group/volume flex items-center">
                <IconButton label={muted ? "Unmute" : "Mute"} onClick={() => setMuted((value) => !value)}>
                  {muted || volume === 0 ? <VolumeX className="size-[18px]" /> : <Volume2 className="size-[18px]" />}
                </IconButton>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={muted ? 0 : volume}
                  aria-label="Volume"
                  onChange={(event) => {
                    setVolume(Number(event.target.value));
                    setMuted(false);
                  }}
                  className="hidden h-1 w-0 cursor-pointer opacity-0 accent-accent transition-all group-hover/volume:ml-1 group-hover/volume:w-20 group-hover/volume:opacity-100 sm:block"
                />
              </div>

              <div className="flex-1" />

              <IconButton
                label="Captions"
                title={subtitles.length > 0 ? "Subtitles" : subtitlesLoading ? "Looking for subtitles" : "No subtitles found"}
                disabled={subtitles.length === 0}
                onClick={() => togglePanel("subtitles")}
              >
                <Captions className="size-[18px]" />
              </IconButton>
              {sources && (
                <IconButton label="Sources" title="Sources" onClick={() => togglePanel("sources")}>
                  <Layers className="size-[18px]" />
                </IconButton>
              )}
              {episodes && (
                <button
                  type="button"
                  onClick={() => togglePanel("episodes")}
                  className="flex h-9 cursor-pointer items-center gap-1.5 rounded-full px-2.5 text-xs font-semibold text-ink transition-colors hover:bg-wash-2"
                >
                  <List className="size-[18px]" />
                  Episodes
                </button>
              )}
              {onNext && (
                <IconButton label="Next episode" onClick={onNext}>
                  <SkipForward className="size-[18px]" />
                </IconButton>
              )}
              <IconButton label="Fullscreen" onClick={toggleFullscreen}>
                {fullscreen ? <Minimize className="size-[18px]" /> : <Maximize className="size-[18px]" />}
              </IconButton>
            </div>
          </div>
        </div>
      </div>

      {offerNext && panel === null && onNext && (
        <div className="gutter absolute bottom-[150px] right-0 flex justify-end">
          <div className="flex items-center gap-4 material-panel rounded-xl px-4 py-3 shadow-[0_0_0_1px_rgb(var(--ink)/0.08),0_24px_60px_rgb(0_0_0/0.55)]">
            <span className="text-sm font-semibold text-ink">Next episode</span>
            <Button look="secondary" icon={<SkipForward className="size-4" />} onClick={onNext}>
              Play now
            </Button>
          </div>
        </div>
      )}

      {episodes && panel === "episodes" && (
        <SidePanel title="Episodes" onClose={() => setPanel(null)}>
          {episodes}
        </SidePanel>
      )}
      {panel === "subtitles" && subtitles.length > 0 && (
        <SidePanel title="Subtitles" onClose={() => setPanel(null)}>
          <CaptionsMenu subtitles={subtitles} active={active} onChoose={choose} />
        </SidePanel>
      )}
      {sources && panel === "sources" && (
        <SidePanel title="Sources" onClose={() => setPanel(null)}>
          {sources({ position: () => video.current?.currentTime ?? 0, close: () => setPanel(null) })}
        </SidePanel>
      )}
    </div>
  );
}

/** The seek bar: buffered behind played, with a handle that grows under the pointer. */
function Seek({
  position,
  duration,
  buffered,
  onSeek,
}: {
  position: number;
  duration: number;
  buffered: number;
  onSeek: (seconds: number) => void;
}) {
  const seekable = duration > 0;
  const played = seekable ? Math.min((position / duration) * 100, 100) : 0;
  const loaded = seekable ? Math.min((buffered / duration) * 100, 100) : 0;
  return (
    <div className="group/seek relative flex h-5 flex-1 items-center">
      <div className="relative h-0.5 w-full rounded-full bg-wash-2 transition-all group-hover/seek:h-1">
        <div className="absolute inset-y-0 left-0 rounded-full bg-ink/30" style={{ width: `${loaded}%` }} />
        <div className="absolute inset-y-0 left-0 rounded-full bg-accent" style={{ width: `${played}%` }} />
        <div
          className="absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-ink opacity-0 transition-opacity group-hover/seek:opacity-100"
          style={{ left: `${played}%` }}
        />
      </div>
      <input
        type="range"
        min={0}
        max={seekable ? duration : 0}
        step={1}
        value={seekable ? position : 0}
        aria-label="Seek"
        disabled={!seekable}
        onChange={(event) => onSeek(Number(event.target.value))}
        className="absolute inset-0 size-full cursor-pointer opacity-0"
      />
    </div>
  );
}
