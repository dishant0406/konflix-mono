"use client";

import Hls from "hls.js";
import {
  Loader2,
  Maximize,
  Minimize,
  Pause,
  Play,
  RotateCcw,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { cn, formatSeconds } from "@/lib/utils";

type Props = {
  /** The playable URL returned by the API. */
  src: string;
  /**
   * "hls" when the API had to convert the container (an MKV the browser cannot
   * play), "file" when the browser can play the bytes directly.
   */
  streamType: string | null;
  title: string;
  /** Reported to the page so it can show the message under the player. */
  onError: (message: string) => void;
  poster?: string;
};

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

// A live HLS playlist has no total length, so the element reports Infinity. That is
// not a duration, and using it makes the seek bar's `max` invalid.
function finiteDuration(value: number): number {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

/**
 * The video element and its controls.
 *
 * Two source kinds need genuinely different handling, and the decision is made
 * from what the API reported rather than guessed from the URL:
 *
 *   hls   the container is not one a browser can play (an MKV release), so the API
 *         serves an HLS playlist. Chrome and Firefox have no native HLS support and
 *         need hls.js. Safari does support it natively, and on iOS hls.js cannot use
 *         MediaSource at all - native playback is also what allows fullscreen and
 *         the system player - so there it is left to Safari.
 *   file  bytes the browser plays on its own. It is set as the element's src.
 *
 * The element is rendered here rather than injected by a player library, so there
 * is exactly one media element and its lifecycle is owned by this component.
 */
export default function VideoPlayer({ src, streamType, title, onError, poster }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const hlsRef = useRef<Hls | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const hideControlsTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [ready, setReady] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [fullscreen, setFullscreen] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(true);

  const isHls = streamType === "hls";

  // The error callback is held in a ref so it is not an effect dependency: a caller
  // passing an inline function would otherwise tear down and rebuild the hls.js
  // instance on every render, restarting the stream behind a permanent spinner.
  const onErrorRef = useRef(onError);
  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);

  // Attaching HLS is an imperative, asynchronous job with its own cleanup, so it
  // is kept out of the render path entirely.
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !src) return undefined;

    let cancelled = false;
    setReady(false);

    if (!isHls) {
      video.src = src;
      return () => {
        video.removeAttribute("src");
        video.load();
      };
    }

    // Safari plays HLS natively, and on iOS MediaSource is unavailable.
    if (!Hls.isSupported()) {
      video.src = src;
      return () => {
        video.removeAttribute("src");
        video.load();
      };
    }

    const hls = new Hls({
      enableWorker: true,
      // A segment is only requested shortly before it is needed, which stops the
      // player asking for parts of an episode ffmpeg has not written yet.
      maxBufferLength: 30,
    });
    hlsRef.current = hls;

    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      if (!cancelled) setReady(true);
    });

    hls.on(Hls.Events.ERROR, (_event, data) => {
      // Only a fatal error means playback has stopped. hls.js recovers from the
      // rest by itself, and reporting those would put an error over a working video.
      if (!data.fatal) return;

      // A manifest that will not load cannot be fixed by loading it again - the
      // usual cause is an expired session, which answers 404 every time. Retrying
      // would loop forever behind a "preparing" message.
      if (data.details === Hls.ErrorDetails.MANIFEST_LOAD_ERROR) {
        hls.destroy();
        hlsRef.current = null;
        onErrorRef.current("This stream has expired. Reload the episode to start it again.");
        return;
      }

      if (data.type === Hls.ErrorTypes.NETWORK_ERROR) hls.startLoad();
      else if (data.type === Hls.ErrorTypes.MEDIA_ERROR) hls.recoverMediaError();
      else {
        hls.destroy();
        hlsRef.current = null;
        onErrorRef.current("Playback failed. Reload the episode to try again.");
      }
    });

    hls.loadSource(src);
    hls.attachMedia(video);

    return () => {
      cancelled = true;
      hls.destroy();
      hlsRef.current = null;
    };
  }, [src, isHls]);

  const togglePlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) video.play().catch(() => onErrorRef.current("Playback could not be started."));
    else video.pause();
  }, []);

  const seekBy = useCallback((delta: number) => {
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = Math.min(Math.max(0, video.currentTime + delta), video.duration || 0);
  }, []);

  const toggleFullscreen = useCallback(async () => {
    const container = containerRef.current;
    if (!container) return;
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await container.requestFullscreen();
    } catch {
      // Fullscreen can be refused (iOS Safari on iPhone does not allow it on an
      // arbitrary element). The native controls still offer it, so this is silent.
    }
  }, []);

  // Keyboard shortcuts, scoped to the player so they never hijack the page.
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const tag = (event.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;

    switch (event.key) {
      case " ":
      case "k":
        event.preventDefault();
        togglePlay();
        break;
      case "ArrowRight":
        event.preventDefault();
        seekBy(5);
        break;
      case "ArrowLeft":
        event.preventDefault();
        seekBy(-5);
        break;
      case "ArrowUp":
        event.preventDefault();
        setVolume((value) => Math.min(1, value + 0.1));
        break;
      case "ArrowDown":
        event.preventDefault();
        setVolume((value) => Math.max(0, value - 0.1));
        break;
      case "m":
        setMuted((value) => !value);
        break;
      case "f":
        void toggleFullscreen();
        break;
      default:
        break;
    }
  };

  useEffect(() => {
    const video = videoRef.current;
    if (video) video.volume = volume;
  }, [volume]);

  useEffect(() => {
    const video = videoRef.current;
    if (video) video.muted = muted;
  }, [muted]);

  useEffect(() => {
    const video = videoRef.current;
    if (video) video.playbackRate = speed;
  }, [speed]);

  useEffect(() => {
    const onFullscreenChange = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener("fullscreenchange", onFullscreenChange);
    return () => document.removeEventListener("fullscreenchange", onFullscreenChange);
  }, []);

  const showControls = () => {
    setControlsVisible(true);
    if (hideControlsTimer.current) clearTimeout(hideControlsTimer.current);
    // Controls stay while paused, so a paused frame never hides the way to resume.
    hideControlsTimer.current = setTimeout(() => {
      if (!videoRef.current?.paused) setControlsVisible(false);
    }, 3000);
  };

  useEffect(() => () => {
    if (hideControlsTimer.current) clearTimeout(hideControlsTimer.current);
  }, []);

  const seekable = Number.isFinite(duration) && duration > 0;
  const progressPercent = seekable ? (currentTime / duration) * 100 : 0;
  const bufferedPercent = seekable ? Math.min((buffered / duration) * 100, 100) : 0;

  return (
    <div
      ref={containerRef}
      onKeyDown={onKeyDown}
      onMouseMove={showControls}
      onMouseLeave={() => !videoRef.current?.paused && setControlsVisible(false)}
      tabIndex={0}
      role="region"
      aria-label={`Video player: ${title}`}
      className="group relative aspect-video w-full overflow-hidden rounded-lg bg-black focus:outline-none"
    >
      <video
        ref={videoRef}
        poster={poster}
        // `playsinline` is required or iOS opens the video in the native player
        // instead of playing inline, which loses the page and the episode list.
        playsInline
        preload="metadata"
        className="h-full w-full"
        onLoadedMetadata={(event) => {
          setDuration(finiteDuration(event.currentTarget.duration));
          setReady(true);
        }}
        // hls.js reports the duration again as the growing playlist advances, so a
        // live remux becomes seekable once the real length is known.
        onDurationChange={(event) => setDuration(finiteDuration(event.currentTarget.duration))}
        onPlay={() => {
          setPlaying(true);
          showControls();
        }}
        onPause={() => {
          setPlaying(false);
          setControlsVisible(true);
        }}
        onTimeUpdate={(event) => {
          const video = event.currentTarget;
          setCurrentTime(video.currentTime);
          if (video.buffered.length > 0) {
            setBuffered(video.buffered.end(video.buffered.length - 1));
          }
        }}
        onWaiting={() => setReady(false)}
        onPlaying={() => setReady(true)}
        onError={(event) => {
          // The common real failure is a codec the browser lacks, and reporting
          // "could not play" with no detail is what makes that look like a broken site.
          const code = (event.currentTarget as HTMLVideoElement).error?.code;
          onErrorRef.current(
            code === 4
              ? "Your browser cannot play this file. The release uses a codec it does not support."
              : "Playback failed. The stream may have expired - reload the episode to try again.",
          );
        }}
        onClick={togglePlay}
      />

      {!ready && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/60">
          <Loader2 className="h-9 w-9 animate-spin text-white" />
          <p className="px-6 text-center text-sm text-white/70">
            {isHls ? "Converting this release for your browser..." : "Buffering..."}
          </p>
        </div>
      )}

      <div
        className={cn(
          "absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/90 to-transparent px-3 pb-2 pt-8 transition-opacity",
          controlsVisible || !playing ? "opacity-100" : "pointer-events-none opacity-0",
        )}
      >
        <div className="relative mb-2 h-1.5 w-full cursor-pointer rounded-full bg-white/25">
          <div className="absolute inset-y-0 left-0 rounded-full bg-white/40" style={{ width: `${bufferedPercent}%` }} />
          <div className="absolute inset-y-0 left-0 rounded-full bg-[#e9376b]" style={{ width: `${progressPercent}%` }} />
          <input
            type="range"
            min={0}
            max={seekable ? duration : 0}
            step={1}
            value={seekable ? currentTime : 0}
            aria-label="Seek"
            onChange={(event) => {
              const video = videoRef.current;
              if (video) video.currentTime = Number(event.target.value);
            }}
            className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
          />
        </div>

        <div className="flex items-center gap-3 text-white">
          <button type="button" onClick={togglePlay} aria-label={playing ? "Pause" : "Play"} className="hover:text-[#e9376b]">
            {playing ? <Pause className="h-5 w-5" /> : <Play className="h-5 w-5" />}
          </button>

          <button type="button" onClick={() => seekBy(-10)} aria-label="Rewind 10 seconds" className="hover:text-[#e9376b]">
            <RotateCcw className="h-4 w-4" />
          </button>

          <span className="min-w-[5.5rem] text-xs tabular-nums">
            {formatSeconds(currentTime)} / {seekable ? formatSeconds(duration) : "--:--"}
          </span>

          <button
            type="button"
            onClick={() => setMuted((value) => !value)}
            aria-label={muted ? "Unmute" : "Mute"}
            className="hover:text-[#e9376b]"
          >
            {muted || volume === 0 ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
          </button>

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
            className="hidden h-1 w-20 cursor-pointer accent-[#e9376b] sm:block"
          />

          <select
            value={speed}
            aria-label="Playback speed"
            onChange={(event) => setSpeed(Number(event.target.value))}
            className="ml-auto cursor-pointer rounded bg-black/50 px-1 py-0.5 text-xs text-white"
          >
            {SPEEDS.map((value) => (
              <option key={value} value={value}>
                {value}x
              </option>
            ))}
          </select>

          <button type="button" onClick={toggleFullscreen} aria-label="Toggle fullscreen" className="hover:text-[#e9376b]">
            {fullscreen ? <Minimize className="h-4 w-4" /> : <Maximize className="h-4 w-4" />}
          </button>
        </div>
      </div>
    </div>
  );
}
