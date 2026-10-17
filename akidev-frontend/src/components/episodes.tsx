"use client";

import { Play } from "lucide-react";
import Link from "next/link";
import { useEffect, useMemo, useRef, useState, type Ref } from "react";

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
 * leaves the show. The panel has its own header, so there is no heading here, only the season
 * tabs; the current episode is scrolled into view when it opens.
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

  // Opening the panel on episode 40 of 60 should show episode 40, not the top of the list.
  const current = useRef<HTMLAnchorElement>(null);
  useEffect(() => {
    if (compact) current.current?.scrollIntoView({ block: "center" });
  }, [compact, season]);

  if (!parsed || parsed.contentType === "movie") return null;

  const heading = !compact && <SectionTitle>Episodes</SectionTitle>;
  const note = (text: string) => <p className={cn("text-sm text-dim", compact && "px-4 py-8 text-center")}>{text}</p>;
  const wrap = (children: React.ReactNode) =>
    compact ? <div className="flex flex-col">{children}</div> : <section className="gutter flex flex-col gap-4 pt-10">{children}</section>;

  if (list?.length) {
    const tabs = seasons.map((each) => (
      <Tab key={each} active={each === season} onClick={() => setChosen(each)} className="shrink-0">
        Season {each}
      </Tab>
    ));
    return wrap(
      <>
        {compact ? (
          // Stays at the top while the rows scroll under it. Only worth showing for more than one season.
          seasons.length > 1 && (
            <div className="no-scrollbar sticky top-0 z-10 flex gap-5 overflow-x-auto bg-surface/80 px-4 pt-1 shadow-[0_1px_0_rgb(var(--ink)/0.07)] backdrop-blur-xl">
              {tabs}
            </div>
          )
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3">
            {heading}
            <div className="flex flex-wrap gap-x-5">{tabs}</div>
          </div>
        )}
        <div className={cn("flex flex-col", compact && "p-2")}>
          {list
            .filter((episode) => episode.season === season)
            .map((episode) => {
              const here = episode.id === currentId;
              return (
                <EpisodeRow
                  key={episode.id}
                  anchor={here ? current : undefined}
                  titleKey={titleKey}
                  episode={episode}
                  current={here}
                  compact={compact}
                />
              );
            })}
        </div>
      </>,
    );
  }

  if (parsed.contentType === "series") return wrap(<>{heading}{note("No episode list is available.")}</>);

  if (count <= 0) return wrap(<>{heading}{note("No episodes have been released yet.")}</>);

  return wrap(
    <>
      {heading}
      <div
        className={cn(
          compact ? "grid grid-cols-[repeat(auto-fill,minmax(3.25rem,1fr))] gap-2 px-4 py-3" : "flex flex-wrap gap-2",
        )}
      >
        {Array.from({ length: count }, (_, index) => {
          const number = index + 1;
          const id = animeEpisodeId(parsed.mediaId, number);
          const selected = id === currentId;
          return (
            <Link
              key={id}
              ref={selected ? current : undefined}
              href={watchPath(titleKey, id)}
              replace={compact}
              aria-current={selected ? "true" : undefined}
              className={cn(
                "flex items-center justify-center rounded-md text-sm font-semibold transition-colors",
                compact ? "h-10" : "h-11 w-14",
                selected ? "bg-accent text-canvas" : "bg-wash text-dim hover:bg-wash-3 hover:text-ink",
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

function EpisodeRow({
  titleKey,
  episode,
  current,
  compact,
  anchor,
}: {
  titleKey: string;
  episode: Episode;
  current: boolean;
  compact: boolean;
  /** Set on the current episode, so the panel can scroll to it. */
  anchor?: Ref<HTMLAnchorElement>;
}) {
  return (
    <Link
      ref={anchor}
      href={watchPath(titleKey, episode.id)}
      replace={compact}
      aria-current={current ? "true" : undefined}
      className={cn(
        "group/episode flex items-center rounded-lg border-b border-ink/[0.06] transition-colors hover:bg-wash",
        compact ? "gap-3 border-b-0 px-3 py-2.5" : "gap-5 px-4 py-3.5",
        current && "bg-wash",
      )}
    >
      <span
        className={cn(
          "shrink-0 font-semibold tabular-nums",
          compact ? "w-8 text-lg" : "w-10 text-[22px]",
          current ? "text-accent" : "text-faint",
        )}
      >
        {episode.number}
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className={cn("truncate font-semibold text-ink", compact ? "text-sm" : "text-[15px]")}>
          {episode.name || `Episode ${episode.number}`}
        </span>
        {episode.airdate && <span className="text-xs text-dim">{episode.airdate.slice(0, 10)}</span>}
      </span>
      {!compact && (
        <Play className="size-4 shrink-0 text-ink opacity-0 transition-opacity group-hover/episode:opacity-100" />
      )}
    </Link>
  );
}
