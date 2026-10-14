"use client";

import { Captions, Search } from "lucide-react";
import Link from "next/link";
import { useEffect, useMemo, useState } from "react";

import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { watchPath } from "@/constants/routes";
import { animeEpisodeId } from "@/lib/media-id";
import { cn } from "@/lib/utils";
import type { MediaEpisode } from "@/types/media";

const PAGE_SIZE = 50;

/**
 * The episode picker.
 *
 * A long-running show has over a thousand episodes, and rendering them all as
 * buttons creates a thousand DOM nodes that each load a poster. Only the selected
 * block of 50 is rendered, which keeps the page responsive for every show.
 *
 * Two shapes are handled. An anime has a flat count, so episodes are numbered 1..N.
 * A series has an explicit list with seasons, so the picker groups by season and
 * links with the season in the id - "S02E05" is a different episode from "S01E05",
 * and the stream layer matches on both.
 */
export default function EpisodeList({
  titleKey,
  totalEpisodes,
  currentEpisodeNumber,
  currentSeason = null,
  episodeList,
}: {
  /** The title's key, e.g. "anime-16498" or "series-169". */
  titleKey: string;
  totalEpisodes: number;
  currentEpisodeNumber: number | null;
  currentSeason?: number | null;
  /** A series' explicit episode list. Absent for an anime, which uses the count. */
  episodeList?: MediaEpisode[] | null;
}) {
  const seasons = useMemo(() => {
    const found = [...new Set((episodeList ?? []).map((episode) => episode.season))];
    return found.sort((a, b) => a - b);
  }, [episodeList]);

  // The season the picker is showing. A series opens on the season being watched,
  // or the first one.
  const [season, setSeason] = useState<number | null>(currentSeason ?? seasons[0] ?? null);
  useEffect(() => {
    if (currentSeason !== null) setSeason(currentSeason);
    else if (seasons.length > 0) setSeason(seasons[0]);
  }, [currentSeason, seasons]);

  const numbered = useMemo(() => {
    if (episodeList?.length) {
      return episodeList
        .filter((episode) => season === null || episode.season === season)
        .map((episode) => ({ number: episode.number, id: episode.episodeId, name: episode.name }));
    }
    // An anime has no explicit list, so the count is expanded.
    return Array.from({ length: Math.max(0, totalEpisodes) }, (_, index) => ({
      number: index + 1,
      id: animeEpisodeId(titleKey.replace(/^[a-z]+-/, ""), index + 1),
      name: null,
    }));
  }, [episodeList, season, totalEpisodes, titleKey]);

  const blocks = useMemo(() => {
    if (numbered.length <= PAGE_SIZE) return [];
    const ranges: string[] = [];
    for (let start = 0; start < numbered.length; start += PAGE_SIZE) {
      const from = numbered[start].number;
      const to = numbered[Math.min(start + PAGE_SIZE - 1, numbered.length - 1)].number;
      ranges.push(`${from}-${to}`);
    }
    return ranges;
  }, [numbered]);

  // Open on the block containing the episode being watched, so the current
  // episode is always visible rather than requiring a scroll through the list.
  const initialBlock = useMemo(() => {
    if (blocks.length === 0) return "";
    if (!currentEpisodeNumber) return blocks[0];
    const index = numbered.findIndex((episode) => episode.number === currentEpisodeNumber);
    return index === -1 ? blocks[0] : blocks[Math.floor(index / PAGE_SIZE)] ?? blocks[0];
  }, [blocks, currentEpisodeNumber, numbered]);

  const [block, setBlock] = useState(initialBlock);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    setBlock(initialBlock);
  }, [initialBlock]);

  const visible = useMemo(() => {
    const [start, end] = (block || "0-0").split("-").map(Number);
    const inBlock = block
      ? numbered.filter((episode) => episode.number >= start && episode.number <= end)
      : numbered;
    const trimmed = filter.trim();
    if (!trimmed) return inBlock;
    return inBlock.filter(
      (episode) =>
        String(episode.number).includes(trimmed) ||
        (episode.name ?? "").toLowerCase().includes(trimmed.toLowerCase()),
    );
  }, [block, filter, numbered]);

  if (totalEpisodes <= 0 && !episodeList?.length) {
    return (
      <div className="rounded-lg border border-border bg-secondary/50 p-6 text-sm text-muted-foreground">
        This title has no episode list yet. Episodes appear once the season is announced.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h3 className="text-lg font-semibold text-foreground">
          Episodes <span className="text-sm font-normal text-muted-foreground">({numbered.length})</span>
        </h3>
        <div className="flex items-center gap-2">
          {seasons.length > 1 && (
            <Select value={String(season)} onValueChange={(value) => setSeason(Number(value))}>
              <SelectTrigger className="w-[9rem]" aria-label="Season">
                <SelectValue placeholder="Season" />
              </SelectTrigger>
              <SelectContent className="z-[120]">
                {seasons.map((value) => (
                  <SelectItem key={value} value={String(value)}>
                    Season {value}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          {blocks.length > 1 && (
            <Select value={block} onValueChange={setBlock}>
              <SelectTrigger className="w-[9rem]" aria-label="Episode range">
                <SelectValue placeholder="Range" />
              </SelectTrigger>
              <SelectContent className="z-[120]">
                {blocks.map((range) => (
                  <SelectItem key={range} value={range}>
                    {range.replace("-", " - ")}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
          <div className="relative">
            <Search className="pointer-events-none absolute inset-y-0 left-2 my-auto h-4 w-4 text-muted-foreground" />
            <Input
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder="Find episode"
              aria-label="Find an episode by number or name"
              className="w-[10rem] pl-8"
            />
          </div>
        </div>
      </div>

      {visible.length === 0 ? (
        <p className="rounded-md bg-secondary/50 px-4 py-6 text-center text-sm text-muted-foreground">
          No episodes match &ldquo;{filter}&rdquo;.
        </p>
      ) : (
        <div className="grid grid-cols-3 gap-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-8">
          {visible.map((episode) => {
            const isCurrent =
              episode.number === currentEpisodeNumber &&
              (currentSeason === null || season === currentSeason);
            // The id is already the provider's own, so a series episode keeps its
            // season. An anime id is rebuilt from the count.
            const id = episode.id ?? animeEpisodeId(titleKey, episode.number);
            return (
              <Link
                key={id}
                href={watchPath(titleKey, id)}
                title={episode.name ?? undefined}
                aria-current={isCurrent ? "true" : undefined}
                className={cn(
                  "flex h-[4.5rem] items-center justify-center gap-1.5 rounded-lg px-1 text-sm font-semibold transition-colors",
                  isCurrent
                    ? "bg-primary text-primary-foreground"
                    : "bg-secondary text-muted-foreground hover:bg-secondary/70 hover:text-foreground",
                )}
              >
                <span>{episode.number}</span>
                <Captions className="h-3.5 w-3.5 shrink-0 opacity-60" aria-hidden />
              </Link>
            );
          })}
        </div>
      )}
    </div>
  );
}
