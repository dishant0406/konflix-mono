"use client";

import { ChevronLeft, ChevronRight } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import { useCallback, useMemo, useState } from "react";

import AnimeRow from "@/components/anime-row";
import EpisodeList from "@/components/anime/episode-list";
import VideoPlayer from "@/components/anime/video-player";
import Container from "@/components/container";
import { Button } from "@/components/ui/button";
import { ROUTES, titlePath, watchPath } from "@/constants/routes";
import { SITE } from "@/constants/site";
import { animeEpisodeId, parseMediaId, seriesEpisodeId } from "@/lib/media-id";
import { mediaPoster, mediaTitle } from "@/lib/utils";
import { useMediaList, useWatchMetadata } from "@/query/use-media";
import { useStreamLink } from "@/query/use-stream-link";

export default function WatchPage() {
  const params = useParams();
  const router = useRouter();

  // The first segment is the title's key; the second is the media id being played.
  // They are the same for a film, and differ only in the episode suffix otherwise.
  const key = typeof params.key === "string" ? params.key : undefined;
  const mediaId = typeof params.id === "string" ? params.id : undefined;
  const parsed = parseMediaId(mediaId);
  const episodeNumber = parsed?.episodeNumber ?? null;
  const season = parsed?.season ?? null;

  // Metadata and the stream are fetched independently on purpose. Metadata takes a
  // few hundred milliseconds; resolving a stream searches torrent indexers and can
  // take much longer. Awaiting both together would leave the page blank the whole
  // time, so the title and episode list render as soon as metadata arrives.
  const metadata = useWatchMetadata(key);
  const stream = useStreamLink(mediaId);
  const [playbackError, setPlaybackError] = useState<string | null>(null);

  const media = metadata.data;
  const title = media ? mediaTitle(media.title) : "";
  const isFilm = parsed?.contentType === "movie";
  const totalEpisodes = media?.episodeList?.length
    ? media.episodeList.length
    : (media?.episodes ??
      (media?.nextAiringEpisode?.episode ? Math.max(media.nextAiringEpisode.episode - 1, 0) : 0));

  // Recommendations are a nicety; a failure here must never affect the player.
  const recommendations = useMediaList(
    { genre: media?.genres?.[0], sort: "popularity", perPage: 12 },
    Boolean(media?.genres?.[0]),
  );

  const handleError = useCallback((message: string) => setPlaybackError(message), []);

  // Moving between episodes must not carry the previous episode's error over. A film
  // has nothing to move to, and a series steps within its season.
  const goToEpisode = (number: number | null) => {
    if (!key || !parsed || !number || number < 1) return;
    setPlaybackError(null);
    const next =
      parsed.contentType === "series"
        ? seriesEpisodeId(parsed.mediaId, season ?? 1, number)
        : animeEpisodeId(parsed.mediaId, number);
    router.push(watchPath(key, next));
  };

  const sourceHost = useMemo(() => {
    if (!stream.data?.url || typeof window === "undefined") return null;
    try {
      return new URL(stream.data.url, window.location.origin).host;
    } catch {
      return null;
    }
  }, [stream.data?.url]);

  const poster = media ? mediaPoster(media.coverImage) : undefined;
  const link = stream.data;

  // The heading. A film is named; an episode is numbered.
  const heading = isFilm ? title : episodeNumber ? `Episode ${episodeNumber}` : "Episode";
  const backHref = key ? titlePath(key) : ROUTES.HOME;

  return (
    <Container className="flex flex-col gap-8 py-6">
      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
          <div className="min-w-0">
            <Link href={backHref} className="text-xs text-muted-foreground hover:text-foreground">
              {title || "Loading..."}
            </Link>
            <h1 className="truncate text-lg font-bold text-foreground md:text-2xl">{heading}</h1>
          </div>

          {!isFilm && (
            <div className="flex shrink-0 gap-2">
              <Button
                variant="secondary"
                disabled={!episodeNumber || episodeNumber <= 1}
                onClick={() => goToEpisode(episodeNumber ? episodeNumber - 1 : null)}
              >
                <ChevronLeft className="mr-1 h-4 w-4" />
                Previous
              </Button>
              <Button
                variant="secondary"
                disabled={!episodeNumber || (totalEpisodes > 0 && episodeNumber >= totalEpisodes)}
                onClick={() => goToEpisode(episodeNumber ? episodeNumber + 1 : null)}
              >
                Next
                <ChevronRight className="ml-1 h-4 w-4" />
              </Button>
            </div>
          )}
        </div>

        {stream.isLoading && (
          <div className="flex aspect-video w-full flex-col items-center justify-center gap-3 rounded-lg bg-background px-6">
            <div className="h-9 w-9 animate-spin rounded-full border-2 border-border border-t-primary" />
            <p className="text-center text-sm text-muted-foreground">
              Searching for a playable source. This can take up to a minute.
            </p>
          </div>
        )}

        {!stream.isLoading && stream.isError && (
          <div className="flex aspect-video w-full flex-col items-center justify-center gap-2 rounded-lg bg-background px-6 text-center">
            <p className="font-bold text-foreground">Could not load this {isFilm ? "film" : "episode"}</p>
            <p className="max-w-lg text-sm text-muted-foreground">
              {stream.error instanceof Error ? stream.error.message : "The API did not respond."}
            </p>
            <Button className="mt-2" onClick={() => stream.refetch()}>
              Try again
            </Button>
          </div>
        )}

        {!stream.isLoading && !stream.isError && link?.available && link.url && (
          <>
            <VideoPlayer
              src={link.url}
              streamType={link.streamType}
              title={link.animeNameWithEP || title}
              poster={poster}
              onError={handleError}
            />
            {playbackError && <p className="text-sm text-ayu-special/80">{playbackError}</p>}
            {link.release && (
              <p className="break-all text-xs text-muted-foreground/80">
                Source: {link.release}
                {sourceHost ? ` · via ${sourceHost}` : ""}
                {link.streamType === "hls" ? " · converted for browser playback" : ""}
              </p>
            )}
          </>
        )}

        {!stream.isLoading && !stream.isError && !link?.available && (
          <div className="flex aspect-video w-full flex-col items-center justify-center gap-3 rounded-lg bg-background px-6 text-center">
            <p className="text-lg font-bold text-foreground">No playable source found</p>
            <p className="max-w-[40rem] text-sm text-muted-foreground">
              {link?.reason || "The API returned no stream for this title."}
            </p>
            <p className="max-w-[40rem] text-xs text-muted-foreground/70">
              Playback depends on public sources, which do not have every release.
            </p>
            {!isFilm && (
              <div className="flex gap-3">
                {episodeNumber && episodeNumber > 1 && (
                  <Button variant="secondary" onClick={() => goToEpisode(episodeNumber - 1)}>
                    Try episode {episodeNumber - 1}
                  </Button>
                )}
                {episodeNumber && episodeNumber < totalEpisodes && (
                  <Button onClick={() => goToEpisode(episodeNumber + 1)}>
                    Try episode {episodeNumber + 1}
                  </Button>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {media && (
        <div className={isFilm ? "" : "grid grid-cols-1 gap-8 lg:grid-cols-4"}>
          {!isFilm && key && (
            <div className="lg:col-span-3">
              <EpisodeList
                animeId={key}
                totalEpisodes={totalEpisodes}
                currentEpisodeNumber={episodeNumber}
                currentSeason={season}
                episodeList={media.episodeList}
              />
            </div>
          )}

          <aside className="flex flex-col gap-4">
            <div className="relative aspect-[2/3] w-full overflow-hidden rounded-lg bg-card">
              {poster && <Image src={poster} alt={title} fill sizes="300px" className="object-cover" />}
            </div>
            <div>
              <h2 className="text-base font-semibold text-foreground">{title}</h2>
              <p className="mt-1 text-xs text-muted-foreground">
                {[media.format, media.status?.replace(/_/g, " "), media.seasonYear]
                  .filter(Boolean)
                  .join(" · ")}
              </p>
            </div>
            <p className="line-clamp-6 whitespace-pre-line text-xs leading-5 text-muted-foreground">
              {media.description}
            </p>
            <Button variant="secondary" asChild>
              <Link href={backHref}>View full details</Link>
            </Button>
          </aside>
        </div>
      )}

      {metadata.isError && (
        <p className="py-10 text-center text-sm text-muted-foreground">
          Could not load this {SITE.itemLabel}. The Konflix API may be unreachable.
        </p>
      )}

      {!metadata.isLoading && !metadata.isError && !media && (
        <p className="py-10 text-center text-sm text-muted-foreground">
          That {SITE.itemLabel} could not be found.{" "}
          <Link href={ROUTES.HOME} className="text-primary hover:underline">
            Back to home
          </Link>
        </p>
      )}

      {recommendations.data?.results?.length ? (
        <AnimeRow
          title={SITE.recommendedTitle}
          media={recommendations.data.results}
          hrefFor={(item) => titlePath(item.id)}
        />
      ) : null}
    </Container>
  );
}
