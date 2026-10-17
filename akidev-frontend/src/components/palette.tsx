"use client";

import { Search } from "lucide-react";
import Image from "next/image";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";

import { KIND_LABEL } from "@/constants/site";
import { searchPath, titlePath } from "@/constants/routes";
import { facts } from "@/lib/media";
import { albumPath, artistPath, musicSearchPath, names, type MusicSearch } from "@/lib/music";
import { useMusicPlayer } from "@/providers/music-player";
import { cn } from "@/lib/utils";
import { useSearch } from "@/query/use-media";
import { useMusicSearch } from "@/query/use-music";

import { MetaLine } from "./states";

const SHOWN = 8;

/** One row of the panel, whichever catalogue it came from. */
interface Item {
  key: string;
  title: string;
  /** The small caps word before the facts: "SERIES", "SONG". */
  label: string;
  facts: string[];
  picture: string | null;
  shape: "poster" | "square" | "round";
  open: () => void;
}

/** The first few of each kind of result, songs first. */
function musicItems(found: MusicSearch | undefined, play: (index: number) => void, go: (path: string) => void): Item[] {
  if (!found) return [];
  return [
    ...found.songs.slice(0, 5).map((song, index) => ({
      key: `song-${song.id}`,
      title: song.title,
      label: "SONG",
      facts: [names(song.artists), ...(song.duration ? [song.duration] : [])],
      picture: song.cover,
      shape: "square" as const,
      open: () => play(index),
    })),
    ...found.artists.slice(0, 2).map((artist) => ({
      key: `artist-${artist.id}`,
      title: artist.name,
      label: "ARTIST",
      facts: artist.subscribers ? [artist.subscribers] : [],
      picture: artist.cover,
      shape: "round" as const,
      open: () => go(artistPath(artist.id)),
    })),
    ...found.albums.slice(0, 3).map((album) => ({
      key: `album-${album.id}`,
      title: album.title,
      label: "ALBUM",
      facts: [album.artist, ...(album.year ? [album.year] : [])],
      picture: album.cover,
      shape: "square" as const,
      open: () => go(albumPath(album.id)),
    })),
  ];
}

/**
 * Search over the page: type, move with the arrows, Enter opens a result or all the results. On the
 * music pages it searches music (a song plays, an album or artist opens); elsewhere, titles.
 */
export default function Palette({ onClose, music = false }: { onClose: () => void; music?: boolean }) {
  const router = useRouter();
  const input = useRef<HTMLInputElement>(null);
  const [typed, setTyped] = useState("");
  const [term, setTerm] = useState("");
  const [selected, setSelected] = useState(0);
  const player = useMusicPlayer();
  // Both are always called; the one not in use is asked for nothing.
  const videos = useSearch(music ? "" : term, SHOWN);
  const songs = useMusicSearch(music ? term : "");

  useEffect(() => input.current?.focus(), []);

  // Wait for a pause in typing, so each keystroke is not a request.
  useEffect(() => {
    const timer = setTimeout(() => setTerm(typed.trim()), 200);
    return () => clearTimeout(timer);
  }, [typed]);

  // The page behind stays put while the panel is open.
  useEffect(() => {
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  }, []);

  const go = (path: string) => {
    onClose();
    router.push(path);
  };
  const results: Item[] = music
    ? musicItems(songs.data, (index) => {
        onClose();
        if (songs.data) player.play(songs.data.songs, index);
      }, go)
    : videos.results.map((card) => ({
        key: card.id,
        title: card.title,
        label: KIND_LABEL[card.kind],
        facts: facts(card),
        picture: card.poster ?? null,
        shape: "poster" as const,
        open: () => go(titlePath(card.id)),
      }));
  // Back to the first row when the list changes (and only then: `results` is rebuilt every render).
  const listed = results.map((item) => item.key).join();
  useEffect(() => setSelected(0), [listed]);
  const stale = music ? songs.isFetching && !!songs.data && songs.isPreviousData : videos.stale;
  const searching = music ? songs.isFetching : videos.searching;
  const failed = music ? songs.isError : videos.failed;

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") onClose();
    else if (event.key === "ArrowDown" && results.length > 0) {
      event.preventDefault();
      setSelected((index) => (index + 1) % results.length);
    } else if (event.key === "ArrowUp" && results.length > 0) {
      event.preventDefault();
      setSelected((index) => (index + results.length - 1) % results.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (results[selected] && !stale && term === typed.trim()) results[selected].open();
      else if (typed.trim()) go(music ? musicSearchPath(typed.trim()) : searchPath(typed.trim()));
    }
  };

  const empty = typed.trim().length === 0;
  const hint = empty
    ? music
      ? "Type to search songs, albums and artists."
      : "Type to search anime, series and films."
    : searching && results.length === 0
      ? "Searching..."
      : failed
        ? "Couldn't reach the catalogue. Check your connection."
        : results.length === 0
          ? "No matches. Press Enter to search everywhere."
          : null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Search"
      className="fixed inset-0 z-50 flex items-start justify-center bg-canvas/70 px-4 pt-[110px] backdrop-blur-sm animate-fade"
      onClick={onClose}
      onKeyDown={onKeyDown}
    >
      <div
        className="flex w-full max-w-[640px] flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-center gap-3 px-4 py-3">
          <Search className="size-[18px] shrink-0 text-dim" />
          <input
            ref={input}
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            placeholder="Search"
            aria-label={music ? "Search songs, albums and artists" : "Search anime, series and films"}
            autoComplete="off"
            spellCheck={false}
            className="w-full bg-transparent text-base text-ink outline-none placeholder:text-faint"
          />
        </div>

        {(hint || results.length > 0) && (
          <div className={cn("flex max-h-[60vh] flex-col overflow-y-auto border-t border-line p-2", stale && "opacity-50")}>
            {results.map((item, index) => (
              <button
                key={item.key}
                type="button"
                onClick={item.open}
                onMouseMove={() => setSelected(index)}
                className={cn(
                  "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-left",
                  index === selected && "bg-raised",
                )}
              >
                <div
                  className={cn(
                    "relative shrink-0 overflow-hidden bg-line",
                    item.shape === "poster" && "h-[54px] w-9 rounded-md",
                    item.shape === "square" && "size-11 rounded-md",
                    item.shape === "round" && "size-11 rounded-full",
                  )}
                >
                  {item.picture && <Image src={item.picture} alt="" fill sizes="44px" className="object-cover" />}
                </div>
                <div className="flex min-w-0 flex-col gap-0.5">
                  <span className="truncate text-[15px] font-semibold text-ink">{item.title}</span>
                  <span className="flex items-center gap-2">
                    <span className="text-[10px] font-bold text-faint">{item.label}</span>
                    <MetaLine parts={item.facts} className="text-xs" />
                  </span>
                </div>
              </button>
            ))}
            {hint && <p className="px-3 py-4 text-sm text-dim">{hint}</p>}
          </div>
        )}
      </div>
    </div>
  );
}
