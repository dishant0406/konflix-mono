"use client";

// Where the viewer got to in each title, so the home page can offer to carry on. Kept in the
// browser, like the app keeps it in a file: the same rules as desktop/app/src/progress.rs.

import { useSyncExternalStore } from "react";

import type { Card } from "@/lib/media";

const KEY = "akidev.progress";
/** How many titles are remembered. */
const KEPT = 20;
/** Before this many seconds there is nothing worth resuming. */
const RESUME_AFTER = 15;
/** Past this fraction the episode counts as watched. */
const FINISHED_AT = 0.93;

export interface Entry {
  media: Card;
  /** The episode or film being watched. */
  episodeId: string;
  position: number;
  duration: number;
  updated: number;
}

export function fraction(entry: Entry): number {
  return entry.duration > 0 ? Math.min(Math.max(entry.position / entry.duration, 0), 1) : 0;
}

let cache: Entry[] | null = null;
const listeners = new Set<() => void>();
const NONE: Entry[] = [];

function read(): Entry[] {
  if (cache) return cache;
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    cache = Array.isArray(parsed) ? parsed : [];
  } catch {
    // Losing a watch history is better than refusing to render.
    cache = [];
  }
  return cache;
}

function write(entries: Entry[]) {
  cache = entries;
  try {
    localStorage.setItem(KEY, JSON.stringify(entries));
  } catch {
    // Storage can be full or blocked; the history then lasts for this visit only.
  }
  listeners.forEach((listener) => listener());
}

/** Remembers where the viewer is. A finished episode is forgotten instead. */
export function record(entry: Omit<Entry, "updated">) {
  const rest = read().filter((existing) => existing.media.id !== entry.media.id);
  const finished = entry.duration > 0 && entry.position >= entry.duration * FINISHED_AT;
  write(finished ? rest : [{ ...entry, updated: Date.now() }, ...rest].slice(0, KEPT));
}

/** Where to start `episodeId`, in seconds, when it was left part way. */
export function resumeAt(episodeId: string): number | null {
  const entry = read().find((existing) => existing.episodeId === episodeId);
  return entry && entry.position > RESUME_AFTER ? entry.position : null;
}

/** What to carry on with, most recent first. Empty on the server and during hydration. */
export function useRecent(): Entry[] {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    read,
    () => NONE,
  );
}
