"use client";

import { ArrowLeft, ArrowRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import MediaCard from "@/components/media-card";
import Container from "@/components/container";
import { Button } from "@/components/ui/button";
import type { MediaList } from "@/types/media";

type Props = {
  title: string;
  media: MediaList[];
  hrefFor: (media: MediaList) => string;
  loading?: boolean;
  episodeLabel?: (media: MediaList) => string | undefined;
};

/**
 * A horizontally scrolling row.
 *
 * The track is a native overflow container rather than a carousel library, so it
 * scrolls by touch, by trackpad and by keyboard for free. The arrows drive the
 * same scroll, which is why they are plain buttons over a real scrollable element.
 */
export default function MediaRow({ title, media, hrefFor, loading = false, episodeLabel }: Props) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);

  const updateArrows = () => {
    const track = trackRef.current;
    if (!track) return;
    setCanScrollLeft(track.scrollLeft > 4);
    setCanScrollRight(track.scrollLeft + track.clientWidth < track.scrollWidth - 4);
  };

  useEffect(() => {
    updateArrows();
    const track = trackRef.current;
    if (!track) return undefined;
    track.addEventListener("scroll", updateArrows, { passive: true });
    window.addEventListener("resize", updateArrows);
    return () => {
      track.removeEventListener("scroll", updateArrows);
      window.removeEventListener("resize", updateArrows);
    };
  }, [media]);

  const scrollBy = (direction: 1 | -1) => {
    const track = trackRef.current;
    if (!track) return;
    track.scrollBy({ left: direction * track.clientWidth * 0.9, behavior: "smooth" });
  };

  if (loading || !media.length) return <RowSkeleton title={title} />;

  return (
    <Container className="flex flex-col gap-5 py-8">
      <div className="flex items-center justify-between gap-4">
        <h2 className="text-xl font-bold text-foreground md:text-2xl">{title}</h2>
        <div className="hidden items-center gap-3 sm:flex">
          <Button
            variant="outline"
            size="icon"
            aria-label={`Scroll ${title} backwards`}
            disabled={!canScrollLeft}
            onClick={() => scrollBy(-1)}
            className="h-9 w-9 rounded-full border-input bg-transparent hover:bg-secondary"
          >
            <ArrowLeft className="h-4 w-4 text-foreground" />
          </Button>
          <Button
            variant="outline"
            size="icon"
            aria-label={`Scroll ${title} forwards`}
            disabled={!canScrollRight}
            onClick={() => scrollBy(1)}
            className="h-9 w-9 rounded-full border-input bg-transparent hover:bg-secondary"
          >
            <ArrowRight className="h-4 w-4 text-foreground" />
          </Button>
        </div>
      </div>

      <div
        ref={trackRef}
        className="flex snap-x snap-mandatory gap-4 overflow-x-auto pb-2 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {media.map((item, index) => (
          <div key={`${item.id}-${index}`} className="shrink-0 snap-start">
            <MediaCard
              media={item}
              href={hrefFor(item)}
              episodeLabel={episodeLabel?.(item)}
              priority={index < 4}
            />
          </div>
        ))}
      </div>
    </Container>
  );
}

function RowSkeleton({ title }: { title: string }) {
  return (
    <Container className="flex flex-col gap-5 py-8">
      <h2 className="text-xl font-bold text-foreground md:text-2xl">{title}</h2>
      <div className="flex gap-4 overflow-hidden">
        {Array.from({ length: 7 }).map((_, index) => (
          <div
            key={index}
            className="h-[16.625rem] w-[11rem] shrink-0 animate-pulse rounded-xl bg-placeholder"
          />
        ))}
      </div>
    </Container>
  );
}
