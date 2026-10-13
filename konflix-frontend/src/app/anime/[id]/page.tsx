"use client";

import { BookmarkCheck, CirclePlay, Tv } from "lucide-react";
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
import { ROUTES } from "@/constants/routes";
import { buildEpisodeId } from "@/lib/episode-id";
import { mediaBanner, mediaPoster, mediaTitle } from "@/lib/utils";
import { useMediaDetail } from "@/query/use-media";
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

export default function AnimeDetailPage() {
  const params = useParams();
  const id = typeof params.id === "string" ? params.id : undefined;
  const { data: media, isLoading, isError, refetch } = useMediaDetail(id);
  const [trailerOpen, setTrailerOpen] = useState(false);

  if (isLoading) return <Loading />;

  // A failed request is not a missing title, and saying "not found" during an
  // outage is both wrong and unactionable.
  if (isError) {
    return (
      <Container className="py-24 text-center">
        <h1 className="text-2xl font-bold text-white">Could not load this anime</h1>
        <p className="mt-2 text-sm text-gray-400">
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
        <h1 className="text-2xl font-bold text-white">Anime not found</h1>
        <p className="mt-2 text-sm text-gray-400">That title is not in the catalogue.</p>
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

  return (
    <div className="w-full">
      <div className="relative h-[30vh] min-h-[14rem] w-full md:h-[40vh]">
        {banner && <Image src={banner} alt="" fill sizes="100vw" priority className="object-cover" />}
        <div className="absolute inset-0 bg-gradient-to-t from-background via-background/60 to-transparent" />
      </div>

      <Container className="pb-20">
        <div className="flex flex-col gap-8 md:flex-row md:items-end md:gap-16">
          <div className="relative -mt-24 h-[18rem] w-[12.5rem] shrink-0 overflow-hidden rounded-xl border border-white/10 bg-slate-900 shadow-2xl md:-mt-32 md:h-[25rem] md:w-[17rem]">
            {poster && <Image src={poster} alt={title} fill sizes="280px" priority className="object-cover" />}
          </div>

          <div className="flex flex-col gap-4 pb-4">
            <h1 className="text-2xl font-extrabold text-white md:text-5xl">{title}</h1>

            <div className="flex flex-wrap items-center gap-2 text-xs">
              {media.format && <Badge className="bg-[#e9376b] text-white">{media.format}</Badge>}
              {media.status && (
                <Badge variant="outline" className="border-gray-600 text-gray-300">
                  {media.status.replace(/_/g, " ")}
                </Badge>
              )}
              {media.seasonYear && (
                <Badge variant="outline" className="border-gray-600 text-gray-300">
                  {media.season ? `${media.season} ` : ""}
                  {media.seasonYear}
                </Badge>
              )}
              {!!media.averageScore && (
                <Badge className="border border-yellow-500/40 bg-yellow-500/20 text-yellow-300">
                  ★ {media.averageScore}%
                </Badge>
              )}
              {totalEpisodes > 0 && (
                <Badge variant="outline" className="border-gray-600 text-gray-300">
                  {totalEpisodes} episodes
                </Badge>
              )}
            </div>

            <div className="flex flex-wrap items-center gap-3">
              <Button asChild className="bg-[#e9376b] text-white hover:bg-[#d32a5d]">
                <Link href={`${ROUTES.WATCH}/${media.id}/${buildEpisodeId(media.id, 1)}`}>
                  <CirclePlay className="mr-2 h-4 w-4" />
                  Watch episode 1
                </Link>
              </Button>
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
            <TabsTrigger value="episodes" className="text-base font-semibold md:text-lg">
              Episodes
            </TabsTrigger>
            {characters.length > 0 && (
              <TabsTrigger value="characters" className="text-base font-semibold md:text-lg">
                Characters
              </TabsTrigger>
            )}
          </TabsList>

          <TabsContent value="overview" className="mt-8 grid w-full grid-cols-1 gap-x-16 gap-y-6 md:grid-cols-5">
            <div className="col-span-1 flex flex-col gap-4">
              <h2 className="text-lg font-semibold text-white">Details</h2>
              <dl className="grid grid-cols-[7rem_1fr] gap-y-2 text-sm">
                <dt className="text-gray-400">Format</dt>
                <dd className="text-gray-200">{media.format ?? "—"}</dd>
                <dt className="text-gray-400">Status</dt>
                <dd className="text-gray-200">{media.status?.replace(/_/g, " ") ?? "—"}</dd>
                <dt className="text-gray-400">Episodes</dt>
                <dd className="text-gray-200">{totalEpisodes || "—"}</dd>
                <dt className="text-gray-400">Duration</dt>
                <dd className="text-gray-200">{media.duration ? `${media.duration} min` : "—"}</dd>
                <dt className="text-gray-400">Studios</dt>
                <dd className="text-gray-200">{studios.join(", ") || "—"}</dd>
                <dt className="text-gray-400">Genres</dt>
                <dd className="text-gray-200">{(media.genres ?? []).join(", ") || "—"}</dd>
              </dl>

              {externalLinks.length > 0 && (
                <div className="flex flex-col gap-2">
                  <h3 className="text-sm font-semibold text-white">Where to watch</h3>
                  <ul className="flex flex-wrap gap-2 text-xs">
                    {externalLinks.slice(0, 8).map((link) => (
                      <li key={link.url}>
                        <a
                          href={link.url as string}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="inline-block rounded border border-white/20 px-2 py-1 text-gray-300 hover:border-[#e9376b] hover:text-white"
                        >
                          {link.site}
                        </a>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>

            <div className="col-span-4 flex flex-col gap-3">
              <h2 className="text-lg font-semibold text-white">Description</h2>
              <p className="whitespace-pre-line text-sm leading-6 text-gray-300">
                {media.description || "No description available."}
              </p>
            </div>
          </TabsContent>

          <TabsContent value="episodes" className="mt-8">
            <EpisodeList animeId={media.id} totalEpisodes={totalEpisodes} currentEpisodeNumber={null} />
          </TabsContent>

          {characters.length > 0 && (
            <TabsContent value="characters" className="mt-8">
              <h2 className="mb-4 text-lg font-semibold text-white">Characters</h2>
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
                {characters.map((edge, index) => {
                  const character = edge.node;
                  const image = character?.image?.large;
                  const voiceActor = edge.voiceActors?.[0]?.name?.full;
                  return (
                    <div key={`${character?.id}-${index}`} className="flex flex-col gap-2">
                      <div className="relative h-[13rem] w-full overflow-hidden rounded-lg bg-slate-900">
                        {image && (
                          <Image
                            src={image}
                            alt={character?.name?.full ?? "Character"}
                            fill
                            sizes="160px"
                            className="object-cover"
                          />
                        )}
                      </div>
                      <p className="line-clamp-1 text-sm font-medium text-white">
                        {character?.name?.full ?? "Unknown"}
                      </p>
                      {voiceActor && <p className="line-clamp-1 text-xs text-gray-400">{voiceActor}</p>}
                    </div>
                  );
                })}
              </div>
            </TabsContent>
          )}
        </Tabs>

        {relations.length > 0 && (
          <AnimeRow
            title="Related"
            media={relations.map(toRowMedia)}
            hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          />
        )}

        {recommendations.length > 0 && (
          <AnimeRow
            title="Recommended"
            media={recommendations.map(toRowMedia)}
            hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
          />
        )}

        <p className="mt-10 flex items-center gap-2 text-xs text-gray-500">
          <BookmarkCheck className="h-3.5 w-3.5" />
          Metadata from AniList. Konflix stores no media.
        </p>
      </Container>

      {trailerOpen && trailerUrl && (
        <div
          className="fixed inset-0 z-[200] flex items-center justify-center bg-black/80 p-4 backdrop-blur"
          onClick={() => setTrailerOpen(false)}
          role="presentation"
        >
          <div className="aspect-video w-full max-w-4xl overflow-hidden rounded-xl border border-white/20">
            <iframe
              src={trailerUrl}
              title={`${title} trailer`}
              allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
              allowFullScreen
              className="h-full w-full"
            />
          </div>
        </div>
      )}
    </div>
  );
}
