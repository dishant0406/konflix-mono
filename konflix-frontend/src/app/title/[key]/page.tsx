"use client";

import { CirclePlay, Tv } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useState } from "react";

import Loading from "@/app/loading";
import AnimeRow from "@/components/anime-row";
import EpisodeList from "@/components/anime/episode-list";
import Container from "@/components/container";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ROUTES, titlePath, watchPath } from "@/constants/routes";
import { SITE } from "@/constants/site";
import { animeEpisodeId, parseMediaId, seriesEpisodeId } from "@/lib/media-id";
import { mediaBanner, mediaPoster, mediaTitle } from "@/lib/utils";
import { useMediaDetail, useMediaList } from "@/query/use-media";
import type { MediaList, MediaRelationNode } from "@/types/media";

// A relation or recommendation is a partial media record, so the missing list
// fields are filled in here. One helper rather than an inline object per row.
function toRowMedia(node: MediaRelationNode): MediaList {
  return {
    id: node.id ?? "",
    title: node.title,
    coverImage: node.coverImage,
    bannerImage: null,
    format: node.format,
    season: null,
    seasonYear: node.startDate?.year ?? null,
    episodes: node.episodes,
    duration: null,
    status: node.status,
    averageScore: node.meanScore ?? null,
    meanScore: node.meanScore,
    popularity: 0,
    favourites: 0,
    genres: null,
    source: null,
    countryOfOrigin: null,
    isAdult: false,
    studios: null,
    nextAiringEpisode: null,
    startDate: node.startDate,
    endDate: null,
  };
}

export default function TitleDetailPage() {
  const params = useParams();
  const key = typeof params.key === "string" ? params.key : undefined;
  const { data: media, isLoading, isError, refetch } = useMediaDetail(key);
  const [trailerOpen, setTrailerOpen] = useState(false);

  if (isLoading) return <Loading />;

  // A failed request is not a missing title, and saying "not found" during an
  // outage is both wrong and unactionable.
  if (isError) {
    return (
      <Container className="py-24 text-center">
        <h1 className="text-2xl font-bold text-foreground">Could not load this {SITE.itemLabel}</h1>
        <p className="mt-2 text-sm text-muted-foreground">
          The Konflix API did not respond. This is usually temporary.
        </p>
        <Button className="mt-6" onClick={() => refetch()}>
          Try again
        </Button>
      </Container>
    );
  }

  if (!media) {
    return (
      <Container className="py-24 text-center">
        <h1 className="text-2xl font-bold text-foreground">Not found</h1>
        <p className="mt-2 text-sm text-muted-foreground">That title is not in the catalogue.</p>
        <Button asChild className="mt-6">
          <Link href={ROUTES.HOME}>Back to home</Link>
        </Button>
      </Container>
    );
  }

  const title = mediaTitle(media.title);
  const poster = mediaPoster(media.coverImage);
  const banner = mediaBanner(media);
  const studios = (media.studios?.nodes ?? []).map((studio) => studio.name).filter(Boolean);
  const parsed = parseMediaId(key);
  const isFilm = parsed?.contentType === "movie";
  const totalEpisodes =
    media.episodes ??
    (media.nextAiringEpisode?.episode ? Math.max(media.nextAiringEpisode.episode - 1, 0) : 0);
  const relations = (media.relations?.edges ?? [])
    .map((edge) => edge.node)
    .filter((node): node is MediaRelationNode => Boolean(node?.id));
  const recommendations = (media.recommendations?.nodes ?? [])
    .map((node) => node.mediaRecommendation)
    .filter((node): node is MediaRelationNode => Boolean(node?.id));
  const characters = (media.characters?.edges ?? []).filter((edge) => edge.node);
  // Only https links are followed: the value comes from the metadata provider, and a
  // non-http scheme would otherwise end up in an href.
  const externalLinks = (media.externalLinks ?? []).filter((link) =>
    link.url?.startsWith("https://"),
  );
  const trailerUrl =
    media.trailer?.site === "youtube" && media.trailer.id
      ? `https://www.youtube.com/embed/${media.trailer.id}?autoplay=1`
      : null;

  // What "watch" means differs by content type: a film is one playable item, an anime
  // counts episodes from 1, and a series addresses them by season and episode.
  const playTarget =
    parsed && key
      ? isFilm
        ? watchPath(key, key)
        : parsed.contentType === "series"
          ? watchPath(key, seriesEpisodeId(parsed.mediaId, 1, 1))
          : watchPath(key, animeEpisodeId(parsed.mediaId, 1))
      : null;
  const playLabel = isFilm ? "Watch now" : "Watch episode 1";

  return (
    <div className="w-full">
      <div className="relative h-[30vh] min-h-[14rem] w-full md:h-[40vh]">
        {banner && <Image src={banner} alt="" fill sizes="100vw" priority className="object-cover" />}
        <div className="absolute inset-0 bg-gradient-to-t from-background via-background/60 to-transparent" />
      </div>

      <Container className="pb-20">
        <div className="flex flex-col gap-8 md:flex-row md:items-end md:gap-16">
          <div className="relative -mt-24 h-[18rem] w-[12.5rem] shrink-0 overflow-hidden rounded-xl border border-border bg-card shadow-2xl md:-mt-32 md:h-[25rem] md:w-[17rem]">
            {poster && <Image src={poster} alt={title} fill sizes="280px" priority className="object-cover" />}
          </div>

          <div className="flex flex-col gap-4 pb-4">
            <h1 className="text-2xl font-extrabold text-foreground md:text-5xl">{title}</h1>

            <div className="flex flex-wrap items-center gap-2 text-xs">
              {media.format && <Badge className="bg-primary text-primary-foreground">{media.format}</Badge>}
              {media.status && (
                <Badge variant="outline" className="border-border text-muted-foreground">
                  {media.status.replace(/_/g, " ")}
                </Badge>
              )}
              {media.seasonYear && (
                <Badge variant="outline" className="border-border text-muted-foreground">
                  {media.season ? `${media.season} ` : ""}
                  {media.seasonYear}
                </Badge>
              )}
              {!!media.averageScore && (
                <Badge className="border border-ayu-yellow/40 bg-ayu-yellow/15 text-ayu-yellow">
                  ★ {media.averageScore}%
                </Badge>
              )}
              {!isFilm && totalEpisodes > 0 && (
                <Badge variant="outline" className="border-border text-muted-foreground">
                  {totalEpisodes} episodes
                </Badge>
              )}
              {isFilm && media.duration ? (
                <Badge variant="outline" className="border-border text-muted-foreground">
                  {media.duration} min
                </Badge>
              ) : null}
            </div>

            <div className="flex flex-wrap items-center gap-3">
              {playTarget && (
                <Button asChild className="bg-primary text-primary-foreground hover:bg-primary/90">
                  <Link href={playTarget}>
                    <CirclePlay className="mr-2 h-4 w-4" />
                    {playLabel}
                  </Link>
                </Button>
              )}
              {trailerUrl && (
                <Button variant="secondary" onClick={() => setTrailerOpen(true)}>
                  <Tv className="mr-2 h-4 w-4" />
                  Trailer
                </Button>
              )}
            </div>
          </div>
        </div>

        <Tabs defaultValue="overview" className="mt-10 w-full">
          <TabsList className="flex h-auto w-fit flex-wrap items-center justify-start">
            <TabsTrigger value="overview" className="text-base font-semibold md:text-lg">
              Overview
            </TabsTrigger>
            {!isFilm && (
              <TabsTrigger value="episodes" className="text-base font-semibold md:text-lg">
                Episodes
              </TabsTrigger>
            )}
            {characters.length > 0 && (
              <TabsTrigger value="characters" className="text-base font-semibold md:text-lg">
                Characters
              </TabsTrigger>
            )}
          </TabsList>

          <TabsContent value="overview" className="mt-8 grid w-full grid-cols-1 gap-x-16 gap-y-6 md:grid-cols-5">
            <div className="col-span-1 flex flex-col gap-4">
              <h2 className="text-lg font-semibold text-foreground">Details</h2>
              <dl className="grid grid-cols-[7rem_1fr] gap-y-2 text-sm">
                <dt className="text-muted-foreground">Format</dt>
                <dd className="text-foreground">{media.format ?? "—"}</dd>
                <dt className="text-muted-foreground">Status</dt>
                <dd className="text-foreground">{media.status?.replace(/_/g, " ") ?? "—"}</dd>
                {!isFilm && (
                  <>
                    <dt className="text-muted-foreground">Episodes</dt>
                    <dd className="text-foreground">{totalEpisodes || "—"}</dd>
                  </>
                )}
                <dt className="text-muted-foreground">Duration</dt>
                <dd className="text-foreground">{media.duration ? `${media.duration} min` : "—"}</dd>
                <dt className="text-muted-foreground">Studio</dt>
                <dd className="text-foreground">{studios.join(", ") || "—"}</dd>
                <dt className="text-muted-foreground">Genres</dt>
                <dd className="text-foreground">{(media.genres ?? []).join(", ") || "—"}</dd>
              </dl>
            </div>

            <div className="col-span-1 flex flex-col gap-4 md:col-span-4">
              <h2 className="text-lg font-semibold text-foreground">Synopsis</h2>
              <p className="whitespace-pre-line text-sm leading-6 text-muted-foreground">
                {media.description || "No synopsis is available for this title."}
              </p>
              {externalLinks.length > 0 && (
                <>
                  <h2 className="mt-4 text-lg font-semibold text-foreground">Links</h2>
                  <div className="flex flex-wrap gap-2">
                    {externalLinks.slice(0, 8).map((link) => (
                      <a
                        key={link.url}
                        href={link.url ?? "#"}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="rounded-md border border-border px-3 py-1 text-xs text-muted-foreground hover:border-input hover:text-foreground"
                      >
                        {link.site || "Link"}
                      </a>
                    ))}
                  </div>
                </>
              )}
            </div>
          </TabsContent>

          {!isFilm && (
            <TabsContent value="episodes" className="mt-8">
              <EpisodeList
                animeId={key ?? ""}
                totalEpisodes={totalEpisodes}
                currentEpisodeNumber={null}
                episodeList={media.episodeList}
              />
            </TabsContent>
          )}

          {characters.length > 0 && (
            <TabsContent value="characters" className="mt-8">
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
                {characters.map((edge, index) => (
                  <div key={`${edge.node?.id}-${index}`} className="flex flex-col items-center gap-2 text-center">
                    <div className="relative h-24 w-24 overflow-hidden rounded-full bg-placeholder">
                      {edge.node?.image?.large && (
                        <Image src={edge.node.image.large} alt="" fill sizes="96px" className="object-cover" />
                      )}
                    </div>
                    <p className="text-xs font-semibold text-foreground">{edge.node?.name?.full}</p>
                    {edge.role && <p className="text-[11px] text-muted-foreground">{edge.role}</p>}
                  </div>
                ))}
              </div>
            </TabsContent>
          )}
        </Tabs>

        {relations.length > 0 && (
          <div className="mt-12">
            <AnimeRow
              title={SITE.relatedTitle}
              media={relations.map(toRowMedia)}
              hrefFor={(item) => titlePath(item.id)}
            />
          </div>
        )}

        {recommendations.length > 0 && (
          <AnimeRow
            title={SITE.recommendedTitle}
            media={recommendations.map(toRowMedia)}
            hrefFor={(item) => titlePath(item.id)}
          />
        )}

        <RelatedByGenre genres={media.genres} />
      </Container>

      {trailerOpen && trailerUrl && (
        <div
          className="fixed inset-0 z-[200] flex items-center justify-center bg-sunk/80 p-4"
          role="dialog"
          aria-modal="true"
          aria-label={`${title} trailer`}
          onClick={() => setTrailerOpen(false)}
        >
          <div className="aspect-video w-full max-w-4xl overflow-hidden rounded-lg bg-sunk">
            <iframe
              src={trailerUrl}
              title={`${title} trailer`}
              className="h-full w-full"
              allow="autoplay; encrypted-media"
              allowFullScreen
            />
          </div>
        </div>
      )}
    </div>
  );
}

// A row drawn from the title's own first genre, so every detail page has something
// below it even when the provider supplies no relations or recommendations (TVMaze
// supplies neither).
function RelatedByGenre({ genres }: { genres: string[] | null }) {
  const genre = genres?.[0];
  const query = useMediaList({ genre, sort: "popularity", perPage: 20 }, Boolean(genre));
  const results = query.data?.results ?? [];
  if (!genre || results.length === 0) return null;

  return (
    <AnimeRow title={`More ${genre}`} media={results} hrefFor={(item) => titlePath(item.id)} />
  );
}
