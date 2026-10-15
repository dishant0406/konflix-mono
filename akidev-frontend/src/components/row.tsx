"use client";

import { ChevronLeft, ChevronRight } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import type { Card } from "@/lib/media";
import { cn } from "@/lib/utils";

import { ErrorState, SectionTitle, Skeleton } from "./states";
import Tile, { TILE_GAP, TILE_WIDTH } from "./tile";

/** Tiles that have their picture before the row has been scrolled: what fits a wide screen, and a little over. */
const FIRST_DRAWN = 10;
/** How far past the visible part of a row pictures are drawn, so they are there as it arrives. */
const AHEAD_PX = 480;
const TILE_STRIDE = TILE_WIDTH + TILE_GAP;

export interface ResumeFor {
  fraction: number;
  href: string;
}

/** A titled row of poster tiles that scrolls sideways. */
export default function MediaRow({
  title,
  items,
  loading,
  failed,
  onRetry,
  resume,
}: {
  title: string;
  items: Card[];
  loading?: boolean;
  failed?: boolean;
  onRetry?: () => void;
  /** Per tile, by index: how far through, and where to carry on. */
  resume?: (ResumeFor | undefined)[];
}) {
  const track = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ left: false, right: false });
  // How many tiles have their picture. A row is far wider than the screen, and the browser's own
  // lazy loading reaches a thousand pixels past it, which is half the row: dozens of pictures per
  // page that nobody has scrolled to. The rest get theirs as the row is scrolled towards them.
  const [drawn, setDrawn] = useState(FIRST_DRAWN);

  const measure = useCallback(() => {
    const element = track.current;
    if (!element) return;
    setEdges({
      left: element.scrollLeft > 4,
      right: element.scrollLeft < element.scrollWidth - element.clientWidth - 4,
    });
    const reached = Math.ceil((element.scrollLeft + element.clientWidth + AHEAD_PX) / TILE_STRIDE);
    setDrawn((current) => Math.max(current, reached));
  }, []);

  useEffect(() => {
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [measure, items.length]);

  const page = (direction: -1 | 1) => {
    const element = track.current;
    if (element) element.scrollBy({ left: direction * element.clientWidth * 0.85, behavior: "smooth" });
  };

  // A row with nothing in it is a heading over a gap, which reads as a broken page.
  if (!loading && !failed && items.length === 0) return null;

  return (
    <section className="flex flex-col gap-3 pt-6" aria-label={title}>
      <SectionTitle className="gutter">{title}</SectionTitle>

      {failed ? (
        <ErrorState title="Couldn't load this row" detail="The catalogue didn't answer. Check your connection." onRetry={onRetry} />
      ) : loading ? (
        <div className="gutter flex gap-3 overflow-hidden">
          {Array.from({ length: 9 }, (_, index) => (
            <Skeleton key={index} className="h-[210px] w-[140px] shrink-0 rounded-lg sm:h-[264px] sm:w-[176px]" />
          ))}
        </div>
      ) : (
        <div className="group/row relative">
          <div
            ref={track}
            onScroll={measure}
            className="no-scrollbar gutter flex gap-3 overflow-x-auto overscroll-x-contain scroll-smooth"
          >
            {items.map((card, index) => (
              <Tile key={card.id} card={card} resume={resume?.[index]} drawn={index < drawn} />
            ))}
          </div>
          <Edge side="left" visible={edges.left} onClick={() => page(-1)} />
          <Edge side="right" visible={edges.right} onClick={() => page(1)} />
        </div>
      )}
    </section>
  );
}

/** The arrow at a row's end, shown while the pointer is over the row and there is more that way. */
function Edge({ side, visible, onClick }: { side: "left" | "right"; visible: boolean; onClick: () => void }) {
  if (!visible) return null;
  const Icon = side === "left" ? ChevronLeft : ChevronRight;
  return (
    <button
      type="button"
      aria-label={side === "left" ? "Scroll left" : "Scroll right"}
      onClick={onClick}
      className={cn(
        "absolute top-0 bottom-8 hidden w-14 cursor-pointer items-center justify-center text-ink opacity-0 transition-opacity group-hover/row:opacity-100 md:flex",
        side === "left" ? "left-0 bg-gradient-to-r from-canvas/90 to-transparent" : "right-0 bg-gradient-to-l from-canvas/90 to-transparent",
      )}
    >
      <Icon className="size-7" />
    </button>
  );
}
