"use client";

import { Info, Play } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useRef } from "react";
import { useQueryClient } from "react-query";

import { KIND_LABEL } from "@/constants/site";
import { titlePath, watchPath } from "@/constants/routes";
import { facts, type Card } from "@/lib/media";
import { cn } from "@/lib/utils";
import { detailQueryOptions } from "@/query/use-media";

import { MetaLine } from "./states";

/** How long the pointer rests on a tile before its page is fetched ahead of the click. */
const RESTS_FOR = 200;

export const TILE_WIDTH = 176;
export const TILE_GAP = 12;

/** A film plays straight away. A show needs its episode list first, so it opens the title. */
export function playHref(card: Card): string {
  return card.kind === "movie" ? watchPath(card.id, card.id) : titlePath(card.id);
}

/**
 * A poster tile: clean at rest, with the title, facts and actions laid over it on hover.
 *
 * `resume` marks a title the viewer is part way through: how far (0 to 1), and the page to
 * carry on at. It draws a progress bar and makes the tile resume rather than open.
 */
export default function Tile({
  card,
  resume,
  priority,
  drawn = true,
  className,
}: {
  card: Card;
  resume?: { fraction: number; href: string };
  priority?: boolean;
  /** False for a tile too far along its row to be worth fetching the picture of yet. */
  drawn?: boolean;
  className?: string;
}) {
  const router = useRouter();
  const client = useQueryClient();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const open = titlePath(card.id);
  const play = resume?.href ?? playHref(card);
  const click = resume ? play : open;

  const rest = () => {
    timer.current = setTimeout(() => {
      // The page and its data, so the click finds both already here and the title appears in place.
      router.prefetch(open);
      void client.prefetchQuery(detailQueryOptions(card.id));
    }, RESTS_FOR);
  };
  const leave = () => {
    if (timer.current) clearTimeout(timer.current);
  };

  return (
    <div
      className={cn("group/tile relative flex w-[140px] shrink-0 flex-col gap-2 sm:w-[176px]", className)}
      onMouseEnter={rest}
      onMouseLeave={leave}
    >
      <div className="relative aspect-[2/3] w-full overflow-hidden rounded-lg border border-transparent bg-raised transition-colors group-hover/tile:border-ink/40">
        {card.poster && drawn ? (
          <Image
            src={card.poster}
            alt=""
            fill
            sizes="176px"
            quality={65}
            priority={priority}
            className="object-cover"
          />
        ) : !card.poster ? (
          <div className="flex size-full items-center justify-center p-4 text-center text-sm font-semibold text-dim">
            {card.title}
          </div>
        ) : null}

        {/* The whole tile is the link; the two round buttons sit above it. */}
        <Link href={click} aria-label={card.title} className="absolute inset-0" />

        <div className="pointer-events-none absolute inset-0 flex flex-col justify-end gap-1.5 bg-gradient-to-t from-canvas/95 via-canvas/0 via-80% to-transparent p-3 opacity-0 transition-opacity group-hover/tile:opacity-100 group-focus-within/tile:opacity-100">
          <span className="text-[10px] font-bold text-accent">{KIND_LABEL[card.kind]}</span>
          <span className="line-clamp-2 text-sm font-bold leading-tight text-ink">{card.title}</span>
          <MetaLine parts={facts(card)} className="text-[11px]" />
          <div className="pointer-events-auto flex gap-2 pt-1">
            <button
              type="button"
              aria-label={`Play ${card.title}`}
              onClick={() => router.push(play)}
              className="flex size-8 cursor-pointer items-center justify-center rounded-full bg-ink text-canvas transition-colors hover:bg-ink/85"
            >
              <Play className="size-3.5 fill-current" />
            </button>
            <button
              type="button"
              aria-label={`About ${card.title}`}
              onClick={() => router.push(open)}
              className="flex size-8 cursor-pointer items-center justify-center rounded-full bg-ink/[0.16] text-ink transition-colors hover:bg-ink/[0.28]"
            >
              <Info className="size-3.5" />
            </button>
          </div>
        </div>

        {resume && (
          <div className="absolute inset-x-0 bottom-0 h-[3px] bg-ink/25">
            <div className="h-full bg-accent" style={{ width: `${Math.round(resume.fraction * 100)}%` }} />
          </div>
        )}
      </div>

      <p className="truncate text-[13px] text-dim">{card.title}</p>
    </div>
  );
}
