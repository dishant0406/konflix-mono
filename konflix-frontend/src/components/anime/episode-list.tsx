"use client";

import { Captions, Search } from "lucide-react";
import Link from "next/link";
import { useEffect, useMemo, useState } from "react";

import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ROUTES } from "@/constants/routes";
import { buildEpisodeId } from "@/lib/episode-id";
import { cn } from "@/lib/utils";

const PAGE_SIZE = 50;

/**
 * The episode picker.
 *
 * A long-running show has over a thousand episodes, and rendering them all as
 * buttons creates a thousand DOM nodes that each load a poster. Only the selected
 * block of 50 is rendered, which keeps the page responsive for every show.
 */
export default function EpisodeList({
  animeId,
  totalEpisodes,
  currentEpisodeNumber,
}: {
  animeId: string;
  totalEpisodes: number;
  currentEpisodeNumber: number | null;
}) {
  const blocks = useMemo(() => {
    const ranges: string[] = [];
    for (let start = 1; start <= totalEpisodes; start += PAGE_SIZE) {
      ranges.push(`${start}-${Math.min(start + PAGE_SIZE - 1, totalEpisodes)}`);
    }
    return ranges;
  }, [totalEpisodes]);

  // Open on the block containing the episode being watched, so the current
  // episode is always visible rather than requiring a scroll through the list.
  const initialBlock = useMemo(() => {
    if (!currentEpisodeNumber) return blocks[0];
    const index = Math.floor((currentEpisodeNumber - 1) / PAGE_SIZE);
    return blocks[index] ?? blocks[0];
  }, [blocks, currentEpisodeNumber]);

  const [block, setBlock] = useState(initialBlock);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    setBlock(initialBlock);
  }, [initialBlock]);

  const episodes = useMemo(() => {
    const [start, end] = (block || "1-0").split("-").map(Number);
    const all = Array.from({ length: Math.max(0, end - start + 1) }, (_, index) => start + index);
    const trimmed = filter.trim();
    if (!trimmed) return all;
    return all.filter((number) => String(number).includes(trimmed));
  }, [block, filter]);

  if (totalEpisodes <= 0) {
    return (
      <div className="rounded-lg border border-white/10 bg-black/30 p-6 text-sm text-gray-400">
        This show has no episode list yet. Episodes appear once the season is announced.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h3 className="text-lg font-semibold text-white">
          Episodes <span className="text-sm font-normal text-gray-400">({totalEpisodes})</span>
        </h3>
        <div className="flex items-center gap-2">
          {blocks.length > 1 && (
            <Select value={block} onValueChange={setBlock}>
              <SelectTrigger className="w-[10rem]" aria-label="Episode range">
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
            <Search className="pointer-events-none absolute inset-y-0 left-2 my-auto h-4 w-4 text-gray-400" />
            <Input
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder="Find episode"
              aria-label="Find an episode by number"
              inputMode="numeric"
              className="w-[10rem] pl-8"
            />
          </div>
        </div>
      </div>

      {episodes.length === 0 ? (
        <p className="rounded-md bg-black/30 px-4 py-6 text-center text-sm text-gray-400">
          No episodes in this range match &ldquo;{filter}&rdquo;.
        </p>
      ) : (
        <div className="grid grid-cols-3 gap-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-8">
          {episodes.map((number) => {
            const isCurrent = number === currentEpisodeNumber;
            return (
              <Link
                key={number}
                href={`${ROUTES.WATCH}/${animeId}/${buildEpisodeId(animeId, number)}`}
                aria-current={isCurrent ? "true" : undefined}
                className={cn(
                  "flex h-[4.5rem] items-center justify-center gap-1.5 rounded-lg text-sm font-semibold transition-colors",
                  isCurrent
                    ? "bg-[#e9376b] text-white"
                    : "bg-secondary text-gray-300 hover:bg-secondary/70 hover:text-white",
                )}
              >
                <span>{number}</span>
                <Captions className="h-3.5 w-3.5 opacity-60" aria-hidden />
              </Link>
            );
          })}
        </div>
      )}
    </div>
  );
}
