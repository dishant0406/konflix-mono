"use client";

import { ExternalLink, Play } from "lucide-react";
import Image from "next/image";

import { Button } from "@/components/button";
import Episodes from "@/components/episodes";
import { stripTags } from "@/lib/text";
import MediaRow from "@/components/row";
import { ErrorState, MetaLine, SectionTitle, Skeleton } from "@/components/states";
import { KIND_LABEL } from "@/constants/site";
import { watchPath } from "@/constants/routes";
import { ApiError } from "@/lib/api";
import { episodeCount, firstPlayable } from "@/lib/episodes";
import { parseMediaId } from "@/lib/media-id";
import { statusLabel } from "@/lib/media";
import { useRecent } from "@/lib/progress";
import { useDetail, useMoreLike } from "@/query/use-media";
import type { Detail } from "@/types/media";

const HEIGHT = "h-[min(700px,max(480px,70vh))]";
/** The banner is at least this tall, and taller when what it says needs the room. */
const MIN_HEIGHT = "min-h-[min(700px,max(480px,70vh))]";

export default function TitleView({ id: key, initial }: { id: string; initial?: Detail }) {
  const parsed = parseMediaId(key);
  const detail = useDetail(key, initial);
  const recent = useRecent();
  const data = detail.data;
  const media = data?.media;
  const moreLike = useMoreLike(parsed?.contentType, media?.genres[0]);

  if (detail.isLoading) {
    return (
      <div>
        <Skeleton className={`w-full rounded-none ${HEIGHT}`} />
      </div>
    );
  }

  if (detail.isError || !data || !media || !key || !parsed) {
    const missing = detail.error instanceof ApiError && detail.error.status === 404;
    return (
      <div className="pt-24">
        {detail.isError && !missing ? (
          <ErrorState title="Couldn't load this title" detail="The catalogue didn't answer. This is usually temporary." onRetry={() => detail.refetch()} />
        ) : (
          <ErrorState title="Not found" detail="That title is not in the catalogue." />
        )}
      </div>
    );
  }

  const { studios, trailer: trailerRef, episodes, cast: people, related, recommended } = data;
  const title = media.title;
  const art = media.banner || media.poster;
  const isFilm = parsed.contentType === "movie";
  const count = episodeCount(data);

  const resume = recent.find((entry) => entry.media.id === key);
  const first = firstPlayable(key, data);
  const play = resume
    ? { id: resume.episodeId, label: "Resume" }
    : first
      ? { id: first, label: "Play" }
      : null;

  const trailer = trailerRef?.site.toLowerCase() === "youtube" ? trailerRef.id : null;

  const facts = [
    media.year,
    media.format?.toLowerCase(),
    media.runtime_minutes ? `${media.runtime_minutes} min` : null,
    statusLabel(media.status),
    !isFilm && count > 0 ? `${count} episodes` : null,
  ]
    .filter((part) => part !== null && part !== undefined && part !== "")
    .map(String);

  const genre = media.genres[0];
  const score = media.score;

  return (
    <div className="pb-8">
      <section className={`relative flex w-full flex-col justify-end overflow-hidden bg-canvas ${MIN_HEIGHT}`}>
        {art && <Image src={art} alt="" fill priority sizes="100vw" className="object-cover" />}
        <div className="absolute inset-0 bg-gradient-to-r from-canvas via-canvas/0 via-[70%] to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent via-50% to-canvas" />

        {/* In the flow, not pinned to the bottom, so a long title or description makes the banner taller
            instead of climbing under the bar. */}
        <div className="gutter relative flex max-w-[calc(640px+5rem)] flex-col items-start gap-3 pb-12 pt-28">
          <div className="flex items-center gap-3">
            <span className="text-xs font-bold text-accent">{KIND_LABEL[parsed.contentType]}</span>
            {!!score && <span className="text-sm font-semibold text-accent">★ {score}%</span>}
          </div>
          <h1 className="line-clamp-3 text-4xl font-black leading-[1.05] text-ink sm:text-5xl md:text-[56px]">{title}</h1>
          <MetaLine parts={facts} />
          {media.description && (
            <p className="line-clamp-4 text-[15px] leading-6 text-dim">{stripTags(media.description).replace(/\s+/g, " ")}</p>
          )}
          {media.genres.length > 0 && (
            <p className="text-[13px] text-faint">{media.genres.slice(0, 4).join(" · ")}</p>
          )}
          {studios.length > 0 && <p className="text-[13px] text-faint">{studios.join(", ")}</p>}
          <div className="flex flex-wrap gap-3 pt-2">
            {play && (
              <Button prefetch href={watchPath(key, play.id)} icon={<Play className="size-4" />}>
                {play.label}
              </Button>
            )}
            {trailer && (
              <a
                href={`https://www.youtube.com/watch?v=${trailer}`}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex shrink-0 items-center justify-center gap-2 rounded-md bg-ink/[0.12] px-5 py-2.5 text-[15px] font-semibold text-ink transition-colors hover:bg-ink/20"
              >
                <ExternalLink className="size-4" />
                Trailer
              </a>
            )}
          </div>
        </div>
      </section>

      <Episodes titleKey={key} count={count} list={episodes} currentId={resume?.episodeId} />

      {people.length > 0 && (
        <section className="gutter flex flex-col gap-4 pt-10">
          <SectionTitle>Cast</SectionTitle>
          <div className="flex flex-wrap gap-x-6 gap-y-4">
            {people.map((person, index) => (
              <div key={`${person.name}-${index}`} className="flex w-full items-center gap-3 sm:w-[230px]">
                <div className="relative size-[52px] shrink-0 overflow-hidden rounded-full bg-ink/10">
                  {person.image && <Image src={person.image} alt="" fill sizes="52px" className="object-cover" />}
                </div>
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold text-ink">{person.name}</p>
                  {person.role && <p className="truncate text-xs text-dim">{person.role}</p>}
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      <MediaRow title="Related" items={related} />
      <MediaRow title="Recommended" items={recommended} />
      {related.length === 0 && recommended.length === 0 && genre && (
        <MediaRow title={`More ${genre}`} items={moreLike.filter((card) => card.id !== key)} />
      )}
    </div>
  );
}

