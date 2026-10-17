"use client";

import { Info, Play } from "lucide-react";
import Image from "next/image";
import { useEffect, useState } from "react";

import { KIND_LABEL } from "@/constants/site";
import { titlePath } from "@/constants/routes";
import { facts, type Card } from "@/lib/media";
import { stripTags } from "@/lib/text";
import { cn } from "@/lib/utils";

import { Button } from "./button";
import { MetaLine, Skeleton } from "./states";
import { playHref } from "./tile";

/** How long each title is featured before the next one fades in. */
const ROTATE_EVERY = 9000;

/** The billboard at the top of the home page: a featured title over its own artwork. */
export default function Hero({ items, loading }: { items: Card[]; loading: boolean }) {
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);
  const count = items.length;

  useEffect(() => {
    if (count < 2 || paused) return undefined;
    const timer = setTimeout(() => setIndex((current) => (current + 1) % count), ROTATE_EVERY);
    return () => clearTimeout(timer);
  }, [index, count, paused]);

  const height = "h-[min(680px,max(460px,64vh))]";

  if (loading && count === 0) return <Skeleton className={cn("w-full rounded-none", height)} />;
  if (count === 0) return <div className="h-24" />;

  const media = items[Math.min(index, count - 1)];

  return (
    <section
      className={cn("relative w-full overflow-hidden bg-canvas", height)}
      aria-label="Featured"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
    >
      <div key={media.id} className="absolute inset-0 animate-fade">
        {media.banner && (
          <Image src={media.banner} alt="" fill priority={index === 0} sizes="100vw" className="object-cover" />
        )}
        {/* One scrim across, so the words read over a busy picture, and one up from the
            bottom, so the banner melts into the page. */}
        <div className="absolute inset-0 bg-gradient-to-r from-canvas via-canvas/0 via-[72%] to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent via-[55%] to-canvas" />

        <div className="gutter absolute inset-x-0 bottom-[72px] flex max-w-[calc(580px+5rem)] flex-col items-start gap-3">
          <div className="flex items-center gap-3">
            <span className="text-xs font-semibold text-accent">{KIND_LABEL[media.kind]}</span>
            <MetaLine parts={facts(media)} />
          </div>
          <h1 className="line-clamp-2 font-title text-4xl font-semibold leading-[1.18] tracking-[-0.02em] text-ink sm:text-5xl md:text-[56px]">
            {media.title}
          </h1>
          {media.description ? (
            <p className="line-clamp-3 text-base leading-[1.45] text-dim">{stripTags(media.description)}</p>
          ) : (
            <p className="text-[15px] text-dim">{media.genres.slice(0, 3).join(" · ")}</p>
          )}
          <div className="flex gap-3 pt-2">
            <Button prefetch href={playHref(media)} icon={<Play className="size-4" />}>
              Play
            </Button>
            <Button prefetch look="secondary" href={titlePath(media.id)} icon={<Info className="size-4" />}>
              More info
            </Button>
          </div>
        </div>
      </div>

      {count > 1 && (
        <div className="gutter absolute inset-x-0 bottom-8 flex items-center justify-end gap-2">
          {items.map((item, dot) => (
            <button
              key={item.id}
              type="button"
              aria-label={`Show ${item.title}`}
              aria-current={dot === index}
              onClick={() => setIndex(dot)}
              className={cn(
                "h-1.5 cursor-pointer rounded-full transition-all",
                dot === index ? "w-6 bg-ink" : "w-1.5 bg-ink/30 hover:bg-ink/70",
              )}
            />
          ))}
        </div>
      )}
    </section>
  );
}
