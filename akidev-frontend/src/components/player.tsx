"use client";

import Hls from "hls.js";
import {
  ChevronLeft,
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
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { cn, formatSeconds } from "@/lib/utils";

import { Button, IconButton } from "./button";
import { Spinner } from "./states";

/** "Next episode" is offered this many seconds before the end. */
const NEXT_OFFER = 40;
/** How long the controls stay after the last movement. */
const IDLE_FOR = 3000;
/** How often the position is reported. */
const REPORT_EVERY = 5000;

// A live HLS playlist has no total length, so the element reports Infinity. That is not a
// duration, and using it makes the seek bar's maths invalid.
function finite(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

type Props = {
  /** The playable URL returned by the API. */
  src: string;
  /** "hls" when the API had to convert the container, "file" when the browser plays it directly. */
  streamType: string | null;
  /** The line in the top bar: title, episode, name. */
  title: string;
  /** Where to start, in seconds, when the viewer left part way. */
  startAt: number | null;
  onBack: () => void;
  onError: (message: string) => void;
  /** Reported every few seconds and when playback pauses, so the home page can offer to carry on. */
  onProgress: (position: number, duration: number) => void;
  /** Absent for a film or the last episode. */
  onNext?: () => void;
  /** The episode list, shown in a side panel from the list button. Absent for a film. */
  episodes?: ReactNode;
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
export default function Player({ src, streamType, title, startAt, onBack, onError, onProgress, onNext, episodes }: Props) {
  const video = useRef<HTMLVideoElement>(null);
  const frame = useRef<HTMLDivElement>(null);
  const idle = useRef<ReturnType<typeof setTimeout> | null>(null);
  const resumed = useRef(false);

  const [ready, setReady] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [visible, setVisible] = useState(true);
  const [panel, setPanel] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const isHls = streamType === "hls";

  // Callbacks are held in refs so a caller passing inline functions cannot tear down and
  // rebuild the stream on every render.
  const callbacks = useRef({ onError, onProgress, onNext });
  useEffect(() => {
    callbacks.current = { onError, onProgress, onNext };
  });

  useEffect(() => {
    const element = video.current;
    if (!element || !src) return undefined;
    let cancelled = false;
    resumed.current = false;
    setReady(false);

    const plain = () => {
      element.src = src;
      return () => {
        element.removeAttribute("src");
        element.load();
      };
    };

    // Safari plays HLS natively, and on iOS MediaSource is unavailable.
    if (!isHls || !Hls.isSupported()) return plain();

    const hls = new Hls({ enableWorker: true, maxBufferLength: 30 });
    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      if (!cancelled) setReady(true);
    });
    hls.on(Hls.Events.ERROR, (_event, data) => {
      // Only a fatal error means playback has stopped; hls.js recovers from the rest.
      if (!data.fatal) return;
      // A manifest that will not load cannot be fixed by loading it again: the usual cause
      // is an expired session, which answers 404 every time.
      if (data.details === Hls.ErrorDetails.MANIFEST_LOAD_ERROR) {
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
    return () => {
      cancelled = true;
      hls.destroy();
    };
  }, [src, isHls]);

  const say = useCallback((text: string) => {
    setMessage(text);
    window.setTimeout(() => setMessage((current) => (current === text ? null : current)), 1400);
  }, []);

  const toggle = useCallback(() => {
    const element = video.current;
    if (!element) return;
    if (element.paused) element.play().catch(() => callbacks.current.onError("Playback could not be started."));
    else element.pause();
  }, []);

  const seekBy = useCallback(
    (delta: number) => {
      const element = video.current;
      if (!element) return;
      element.currentTime = Math.min(Math.max(0, element.currentTime + delta), element.duration || 0);
      say(`${delta > 0 ? "+" : "−"}${Math.abs(delta)}s`);
    },
    [say],
  );

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
          case "f":
            void toggleFullscreen();
            return true;
          case "Escape":
            if (panel) setPanel(false);
            return panel;
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
  }, [panel, seekBy, toggle, toggleFullscreen, wake]);

  const remaining = Math.max(duration - position, 0);
  const offerNext = Boolean(onNext) && duration > 0 && remaining <= NEXT_OFFER;
  const shown = visible || !playing || panel;

  return (
    <div
      ref={frame}
      onMouseMove={wake}
      onTouchStart={wake}
      className={cn("fixed inset-0 z-[60] overflow-hidden bg-black", !shown && "cursor-none")}
    >
      <video
        ref={video}
        playsInline
        preload="metadata"
        className="size-full"
        onClick={toggle}
        onDoubleClick={toggleFullscreen}
        onLoadedMetadata={(event) => {
          const element = event.currentTarget;
          setDuration(finite(element.duration));
          setReady(true);
          if (!resumed.current && startAt) element.currentTime = startAt;
          resumed.current = true;
        }}
        onDurationChange={(event) => setDuration(finite(event.currentTarget.duration))}
        onPlay={() => {
          setPlaying(true);
          wake();
        }}
        onPause={() => {
          setPlaying(false);
          setVisible(true);
          const element = video.current;
          if (element && finite(element.duration) > 0) callbacks.current.onProgress(element.currentTime, element.duration);
        }}
        onTimeUpdate={(event) => {
          const element = event.currentTarget;
          setPosition(element.currentTime);
          if (element.buffered.length > 0) setBuffered(element.buffered.end(element.buffered.length - 1));
        }}
        onWaiting={() => setReady(false)}
        onPlaying={() => setReady(true)}
        onEnded={() => callbacks.current.onNext?.()}
        onError={(event) => {
          // The common real failure is a codec the browser lacks; "could not play" with no
          // detail is what makes that look like a broken site.
          const code = event.currentTarget.error?.code;
          callbacks.current.onError(
            code === 4
              ? "Your browser cannot play this file. The release uses a codec it does not support."
              : "Playback failed. The stream may have expired - reload the episode to try again.",
          );
        }}
      />

      {!ready && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2">
          <Spinner />
          <p className="text-sm font-semibold text-ink">{isHls ? "Converting for your browser" : "Buffering"}</p>
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
            <p className="min-w-0 truncate text-[15px] font-semibold text-ink">{title}</p>
          </div>
        </div>

        <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-canvas/90 via-canvas/60 to-transparent pt-16">
          <div className="gutter flex flex-col gap-1 pb-4">
            <div className="flex items-center gap-3 text-xs tabular-nums text-ink">
              <span className="w-12 text-right">{formatSeconds(position)}</span>
              <Seek position={position} duration={duration} buffered={buffered} onSeek={(to) => video.current && (video.current.currentTime = to)} />
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

              {episodes && (
                <button
                  type="button"
                  onClick={() => setPanel((open) => !open)}
                  className="flex h-9 cursor-pointer items-center gap-1.5 rounded-full px-2.5 text-xs font-semibold text-ink transition-colors hover:bg-ink/[0.14]"
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

      {offerNext && !panel && onNext && (
        <div className="gutter absolute bottom-[150px] right-0 flex justify-end">
          <div className="flex items-center gap-4 rounded-xl border border-line bg-surface/95 px-4 py-3">
            <span className="text-sm font-semibold text-ink">Next episode</span>
            <Button look="secondary" icon={<SkipForward className="size-4" />} onClick={onNext}>
              Play now
            </Button>
          </div>
        </div>
      )}

      {episodes && panel && (
        <aside className="absolute bottom-28 right-0 top-14 flex w-full max-w-[400px] flex-col border-l border-line bg-surface/[0.97]">
          <div className="flex items-center justify-between px-4 py-3">
            <h2 className="text-base font-bold text-ink">Episodes</h2>
            <IconButton label="Close episodes" onClick={() => setPanel(false)}>
              <X className="size-[18px]" />
            </IconButton>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-6">{episodes}</div>
        </aside>
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
      <div className="relative h-0.5 w-full rounded-full bg-ink/[0.14] transition-all group-hover/seek:h-1">
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
