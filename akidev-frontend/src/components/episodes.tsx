"use client";

import { Play } from "lucide-react";
import Link from "next/link";
import { useMemo, useState } from "react";

import { watchPath } from "@/constants/routes";
import { animeEpisodeId, parseMediaId } from "@/lib/media-id";
import { cn } from "@/lib/utils";
import type { Episode } from "@/types/media";

import { Tab } from "./button";
import { SectionTitle } from "./states";

/**
 * The episode picker. A series lists its episodes by season, with names and air dates; an
 * anime has a flat count, so its episodes are numbered tiles. A film has none.
 *
 * `compact` is the watch page's side panel: it swaps the episode in place, so Back still
 * leaves the show, and drops the heading.
 */
export default function Episodes({
  titleKey,
  count,
  list,
  currentId,
  compact = false,
}: {
  titleKey: string;
  /** An anime's episode count. */
  count: number;
  /** A series' explicit list. */
  list?: Episode[] | null;
  currentId?: string | null;
  compact?: boolean;
}) {
  const parsed = parseMediaId(titleKey);
  const seasons = useMemo(
    () => [...new Set((list ?? []).map((episode) => episode.season))].sort((a, b) => a - b),
    [list],
  );
  const currentSeason = list?.find((episode) => episode.id === currentId)?.season;
  const [chosen, setChosen] = useState<number | null>(null);
  const season = chosen ?? currentSeason ?? seasons[0];

  if (!parsed || parsed.contentType === "movie") return null;

  const heading = !compact && <SectionTitle>Episodes</SectionTitle>;
  const note = (text: string) => <p className="text-sm text-dim">{text}</p>;
  const wrap = (children: React.ReactNode) => (
    <section className={cn("flex flex-col gap-4", !compact && "gutter pt-10")}>{children}</section>
  );

  if (list?.length) {
    return wrap(
      <>
        <div className="flex flex-wrap items-center justify-between gap-3">
          {heading}
          <div className="flex flex-wrap gap-x-5">
            {seasons.map((each) => (
              <Tab key={each} active={each === season} onClick={() => setChosen(each)}>
                Season {each}
              </Tab>
            ))}
          </div>
        </div>
        <div className="flex flex-col">
          {list
            .filter((episode) => episode.season === season)
            .map((episode) => (
              <EpisodeRow key={episode.id} titleKey={titleKey} episode={episode} current={episode.id === currentId} />
            ))}
        </div>
      </>,
    );
  }

  if (parsed.contentType === "series") return wrap(<>{heading}{note("No episode list is available.")}</>);

  if (count <= 0) return wrap(<>{heading}{note("No episodes have been released yet.")}</>);

  return wrap(
    <>
      {heading}
      <div className="flex flex-wrap gap-2">
        {Array.from({ length: count }, (_, index) => {
          const number = index + 1;
          const id = animeEpisodeId(parsed.mediaId, number);
          const selected = id === currentId;
          return (
            <Link
              key={id}
              href={watchPath(titleKey, id)}
              replace={compact}
              aria-current={selected ? "true" : undefined}
              className={cn(
                "flex h-11 w-14 items-center justify-center rounded-md text-sm font-semibold transition-colors",
                selected ? "bg-ink text-canvas" : "bg-ink/[0.08] text-dim hover:bg-ink/[0.16] hover:text-ink",
              )}
            >
              {number}
            </Link>
          );
        })}
      </div>
    </>,
  );
}

function EpisodeRow({ titleKey, episode, current }: { titleKey: string; episode: Episode; current: boolean }) {
  return (
    <Link
      href={watchPath(titleKey, episode.id)}
      className={cn(
        "group/episode flex items-center gap-5 rounded-lg border-b border-ink/[0.06] px-4 py-3.5 transition-colors hover:bg-ink/[0.07]",
        current && "bg-ink/[0.07]",
      )}
    >
      <span className={cn("w-10 shrink-0 text-[22px] font-bold", current ? "text-accent" : "text-faint")}>
        {episode.number}
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-[15px] font-semibold text-ink">{episode.name || `Episode ${episode.number}`}</span>
        {episode.airdate && <span className="text-xs text-dim">{episode.airdate.slice(0, 10)}</span>}
      </span>
      <Play className="size-4 shrink-0 text-ink opacity-0 transition-opacity group-hover/episode:opacity-100" />
    </Link>
  );
}
