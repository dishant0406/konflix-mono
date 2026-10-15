"use client";

import { ExternalLink, Play } from "lucide-react";
import Image from "next/image";
import { useParams } from "next/navigation";

import { Button } from "@/components/button";
import Episodes from "@/components/episodes";
import { stripTags } from "@/components/hero";
import MediaRow from "@/components/row";
import { ErrorState, MetaLine, SectionTitle, Skeleton } from "@/components/states";
import { KIND_LABEL } from "@/constants/site";
import { watchPath } from "@/constants/routes";
import { relationCard } from "@/lib/media";
import { animeEpisodeId, parseMediaId } from "@/lib/media-id";
import { useRecent } from "@/lib/progress";
import { mediaBanner, mediaPoster, mediaTitle } from "@/lib/utils";
import { useDetail, useMoreLike } from "@/query/use-media";
import type { MediaFull } from "@/types/media";

const HEIGHT = "h-[min(700px,max(480px,70vh))]";

const STATUS: Record<string, string> = {
  FINISHED: "finished",
  RELEASING: "airing",
  NOT_YET_RELEASED: "upcoming",
  CANCELLED: "cancelled",
  HIATUS: "on hiatus",
};

/** What plays when Play is pressed: the film itself, or the first episode. */
function firstPlayable(key: string, media: MediaFull): string | null {
  const parsed = parseMediaId(key);
  if (!parsed) return null;
  if (parsed.contentType === "movie") return key;
  if (parsed.contentType === "series") return media.episodeList?.[0]?.episodeId ?? null;
  return animeEpisodeId(parsed.mediaId, 1);
}

export default function TitlePage() {
  const params = useParams();
  const key = typeof params.key === "string" ? params.key : undefined;
  const parsed = parseMediaId(key);
  const detail = useDetail(key);
  const recent = useRecent();
  const media = detail.data;
  const moreLike = useMoreLike(parsed?.contentType, media?.genres?.[0]);

  if (detail.isLoading) {
    return (
      <div>
        <Skeleton className={`w-full rounded-none ${HEIGHT}`} />
      </div>
    );
  }

  if (detail.isError || !media || !key || !parsed) {
    return (
      <div className="pt-24">
        {detail.isError ? (
          <ErrorState title="Couldn't load this title" detail="The catalogue didn't answer. This is usually temporary." onRetry={() => detail.refetch()} />
        ) : (
          <ErrorState title="Not found" detail="That title is not in the catalogue." />
        )}
      </div>
    );
  }

  const title = mediaTitle(media.title);
  const art = mediaBanner(media) || mediaPoster(media.coverImage);
  const studios = (media.studios?.nodes ?? []).map((studio) => studio.name).filter(Boolean) as string[];
  const isFilm = parsed.contentType === "movie";
  const count =
    media.episodes ?? (media.nextAiringEpisode?.episode ? Math.max(media.nextAiringEpisode.episode - 1, 0) : 0);

  const resume = recent.find((entry) => entry.media.id === key);
  const first = firstPlayable(key, media);
  const play = resume
    ? { id: resume.episodeId, label: "Resume" }
    : first
      ? { id: first, label: "Play" }
      : null;

  const trailer = media.trailer?.site === "youtube" ? media.trailer.id : null;

  const facts = [
    media.seasonYear ?? media.startDate?.year,
    media.format?.toLowerCase(),
    media.duration ? `${media.duration} min` : null,
    media.status ? STATUS[media.status] : null,
    !isFilm && count > 0 ? `${count} episodes` : null,
  ]
    .filter((part) => part !== null && part !== undefined && part !== "")
    .map(String);

  const people = (media.characters?.edges ?? []).flatMap((edge) =>
    edge.node?.name?.full ? [{ name: edge.node.name.full, role: edge.role, image: edge.node.image?.large ?? null }] : [],
  );
  const related = (media.relations?.edges ?? []).flatMap((edge) => {
    const card = edge.node ? relationCard(edge.node) : null;
    return card ? [card] : [];
  });
  const recommended = (media.recommendations?.nodes ?? []).flatMap((node) => {
    const card = node.mediaRecommendation ? relationCard(node.mediaRecommendation) : null;
    return card ? [card] : [];
  });
  const genre = media.genres?.[0];
  const score = media.averageScore;

  return (
    <div className="pb-8">
      <section className={`relative w-full overflow-hidden bg-canvas ${HEIGHT}`}>
        {art && <Image src={art} alt="" fill priority sizes="100vw" className="object-cover" />}
        <div className="absolute inset-0 bg-gradient-to-r from-canvas via-canvas/0 via-[70%] to-transparent" />
        <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent via-50% to-canvas" />

        <div className="gutter absolute inset-x-0 bottom-12 flex max-w-[calc(640px+5rem)] flex-col items-start gap-3">
          <div className="flex items-center gap-3">
            <span className="text-xs font-bold text-accent">{KIND_LABEL[parsed.contentType]}</span>
            {!!score && <span className="text-sm font-semibold text-accent">★ {score}%</span>}
          </div>
          <h1 className="line-clamp-3 text-4xl font-black leading-[1.05] text-ink sm:text-5xl md:text-[56px]">{title}</h1>
          <MetaLine parts={facts} />
          {media.description && (
            <p className="line-clamp-4 text-[15px] leading-6 text-dim">{stripTags(media.description)}</p>
          )}
          {media.genres && media.genres.length > 0 && (
            <p className="text-[13px] text-faint">{media.genres.slice(0, 4).join(" · ")}</p>
          )}
          {studios.length > 0 && <p className="text-[13px] text-faint">{studios.join(", ")}</p>}
          <div className="flex flex-wrap gap-3 pt-2">
            {play && (
              <Button href={watchPath(key, play.id)} icon={<Play className="size-4" />}>
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

      <Episodes titleKey={key} count={count} list={media.episodeList} currentId={resume?.episodeId} />

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

