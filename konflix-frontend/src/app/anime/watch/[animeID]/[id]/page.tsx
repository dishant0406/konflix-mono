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
import { ROUTES } from "@/constants/routes";
import { buildEpisodeId, parseEpisodeId } from "@/lib/episode-id";
import { mediaPoster, mediaTitle } from "@/lib/utils";
import { useMediaList, useWatchMetadata } from "@/query/use-media";
import { useStreamLink } from "@/query/use-stream-link";

export default function WatchPage() {
  const params = useParams();
  const router = useRouter();

  const animeId = typeof params.animeID === "string" ? params.animeID : undefined;
  const episodeId = typeof params.id === "string" ? params.id : undefined;
  const parsed = parseEpisodeId(episodeId);
  const episodeNumber = parsed?.episodeNumber ?? null;

  // Metadata and the stream are fetched independently on purpose. Metadata takes a
  // few hundred milliseconds; resolving a stream searches torrent indexers and can
  // take much longer. Awaiting both together would leave the page blank the whole
  // time, so the title and episode list render as soon as metadata arrives.
  const metadata = useWatchMetadata(animeId);
  const stream = useStreamLink(episodeId);
  const [playbackError, setPlaybackError] = useState<string | null>(null);

  const media = metadata.data;
  const title = media ? mediaTitle(media.title) : "";
  const totalEpisodes =
    media?.episodes ??
    (media?.nextAiringEpisode?.episode ? Math.max(media.nextAiringEpisode.episode - 1, 0) : 0);

  // Recommendations are a nicety; a failure here must never affect the player.
  const recommendations = useMediaList(
    { genre: media?.genres?.[0], sort: "trending", perPage: 12 },
    Boolean(media?.genres?.[0]),
  );

  const handleError = useCallback((message: string) => setPlaybackError(message), []);

  // Moving between episodes must not carry the previous episode's error over.
  const goToEpisode = (number: number | null) => {
    if (!animeId || !number) return;
    setPlaybackError(null);
    router.push(`${ROUTES.WATCH}/${animeId}/${buildEpisodeId(animeId, number)}`);
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

  return (
    <Container className="flex flex-col gap-8 py-6">
      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
          <div className="min-w-0">
            <Link href={`${ROUTES.ANIME_DETAILS}/${animeId}`} className="text-xs text-gray-400 hover:text-white">
              {title || "Loading..."}
            </Link>
            <h1 className="truncate text-lg font-bold text-white md:text-2xl">
              {episodeNumber ? `Episode ${episodeNumber}` : "Episode"}
            </h1>
          </div>

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
        </div>

        {stream.isLoading && (
          <div className="flex aspect-video w-full flex-col items-center justify-center gap-3 rounded-lg bg-black px-6">
            <div className="h-9 w-9 animate-spin rounded-full border-2 border-white/20 border-t-white" />
            <p className="text-center text-sm text-white/60">
              Searching for a playable source. This can take up to a minute.
            </p>
          </div>
        )}

        {!stream.isLoading && stream.isError && (
          <div className="flex aspect-video w-full flex-col items-center justify-center gap-2 rounded-lg bg-black px-6 text-center">
            <p className="font-bold text-white">Could not load this episode</p>
            <p className="max-w-lg text-sm text-white/60">
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
            {playbackError && <p className="text-sm text-amber-300/80">{playbackError}</p>}
            {link.release && (
              <p className="break-all text-xs text-white/50">
                Source: {link.release}
                {sourceHost ? ` · via ${sourceHost}` : ""}
                {link.streamType === "hls" ? " · converted for browser playback" : ""}
              </p>
            )}
          </>
        )}

        {!stream.isLoading && !stream.isError && !link?.available && (
          <div className="flex aspect-video w-full flex-col items-center justify-center gap-3 rounded-lg bg-black px-6 text-center">
            <p className="text-lg font-bold text-white">No playable source found</p>
            <p className="max-w-[40rem] text-sm text-white/60">
              {link?.reason || "The API returned no stream for this episode."}
            </p>
            <p className="max-w-[40rem] text-xs text-white/40">
              Playback depends on public sources, which do not have every episode.
            </p>
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
          </div>
        )}
      </div>

      {media && (
        <div className="grid grid-cols-1 gap-8 lg:grid-cols-4">
          <div className="lg:col-span-3">
            <EpisodeList animeId={media.id} totalEpisodes={totalEpisodes} currentEpisodeNumber={episodeNumber} />
          </div>

          <aside className="flex flex-col gap-4">
            <div className="relative aspect-[2/3] w-full overflow-hidden rounded-lg bg-slate-900">
              {poster && <Image src={poster} alt={title} fill sizes="300px" className="object-cover" />}
            </div>
            <div>
              <h2 className="text-base font-semibold text-white">{title}</h2>
              <p className="mt-1 text-xs text-gray-400">
                {[media.format, media.status?.replace(/_/g, " "), media.seasonYear].filter(Boolean).join(" · ")}
              </p>
            </div>
            <p className="line-clamp-6 whitespace-pre-line text-xs leading-5 text-gray-400">
              {media.description}
            </p>
            <Button variant="secondary" asChild>
              <Link href={`${ROUTES.ANIME_DETAILS}/${media.id}`}>View full details</Link>
            </Button>
          </aside>
        </div>
      )}

      {metadata.isError && (
        <p className="py-10 text-center text-sm text-gray-400">
          Could not load this anime. The Konflix API may be unreachable.
        </p>
      )}

      {!metadata.isLoading && !metadata.isError && !media && (
        <p className="py-10 text-center text-sm text-gray-400">
          That anime could not be found.{" "}
          <Link href={ROUTES.HOME} className="text-[#e9376b] hover:underline">
            Back to home
          </Link>
        </p>
      )}

      {recommendations.data?.results?.length ? (
        <AnimeRow
          title="More like this"
          media={recommendations.data.results}
          hrefFor={(item) => `${ROUTES.ANIME_DETAILS}/${item.id}`}
        />
      ) : null}
    </Container>
  );
}
