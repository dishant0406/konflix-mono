// Keeping a guest's player in step with the host's. The same rules, and the same tests, as
// desktop/party/src/sync.rs.

import type { Item } from "./protocol.ts";

/** A gap bigger than this is fixed with a seek; a smaller one by running a little faster or slower. */
const SEEK_ABOVE = 0.8;
/** A gap smaller than this is not worth correcting. */
const LEAVE_BELOW = 0.15;
/** How much faster or slower to run to close a small gap, which is not noticeable. */
const NUDGE = 0.05;
/**
 * However long ago the host's clock says it spoke, a guest moves its word on by no more than this
 * many seconds: a clock that is far out must not send a guest far from the picture.
 */
const MAX_LATE = 10;

/** What is playing, and how far in. */
export interface Playback {
  item: Item | null;
  playing: boolean;
  position: number;
}

/** What a guest should be playing, with the position where the host was when it was received. */
export interface Target {
  playback: Playback;
  /** `performance.now()` when it was received. */
  received: number;
}

/** Where the host is now: the position it reported, moved on by the time since. */
export function positionNow(target: Target, now = performance.now()): number {
  return target.playback.playing ? target.playback.position + (now - target.received) / 1000 : target.playback.position;
}

/** What a guest does to its player to catch up: a seek, or a playback speed (1 is normal). */
export interface Fix {
  seek: number | null;
  speed: number;
}

/** How to bring a guest at `local` seconds to where the host is, `target`. */
export function fix(local: number, target: number, playing: boolean): Fix {
  const gap = local - target;
  if (!playing) return { seek: Math.abs(gap) > LEAVE_BELOW ? target : null, speed: 1 };
  if (Math.abs(gap) > SEEK_ABOVE) return { seek: target, speed: 1 };
  if (Math.abs(gap) < LEAVE_BELOW) return { seek: null, speed: 1 };
  return { seek: null, speed: gap > 0 ? 1 - NUDGE : 1 + NUDGE };
}

/**
 * The difference between this device's clock and the host's, from the quickest round trip seen
 * (the quickest is the least disturbed, so the closest to the truth).
 */
export class Clock {
  private best: { roundTrip: number; offset: number } | null = null;

  /** A `pong`: sent at `sent` (this clock), answered at `at` (host clock), received at `now`. */
  sample(sent: number, at: number, now: number) {
    const roundTrip = Math.max(0, now - sent);
    // The host read its clock about half way.
    const offset = at + Math.floor(roundTrip / 2) - now;
    if (!this.best || roundTrip <= this.best.roundTrip) this.best = { roundTrip, offset };
  }

  /** Whether a `pong` has been seen, so the clocks are known. */
  get known(): boolean {
    return this.best !== null;
  }

  /** The host's clock now, when this device's reads `now`. */
  hostTime(now: number): number {
    return Math.max(0, now + (this.best?.offset ?? 0));
  }
}

/** What the host said, moved on by how long ago it said it. */
export function targetOf(
  state: { item: Item | null; playing: boolean; position: number; at: number },
  clock: Clock,
  now: number,
  received = performance.now(),
): Target {
  const late = Math.min(MAX_LATE, Math.max(0, clock.hostTime(now) - state.at) / 1000);
  return {
    playback: { item: state.item, playing: state.playing, position: state.playing ? state.position + late : state.position },
    received,
  };
}
