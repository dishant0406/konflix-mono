"use client";

import { useParams, useRouter } from "next/navigation";
import { useCallback, useMemo, useState } from "react";

import { Button } from "@/components/button";
import Episodes from "@/components/episodes";
import Player from "@/components/player";
import { ErrorState, Spinner } from "@/components/states";
import { ROUTES, titlePath, watchPath } from "@/constants/routes";
import { episodeCount, neighbour, titleLine } from "@/lib/episodes";
import { parseMediaId } from "@/lib/media-id";
import { playErrorMessage } from "@/lib/play";
import { record, resumeAt } from "@/lib/progress";
import { useDetail } from "@/query/use-media";
import { usePlay } from "@/query/use-play";

export default function WatchPage() {
  const params = useParams();
  const router = useRouter();
  const key = typeof params.key === "string" ? params.key : undefined;
  const id = typeof params.id === "string" ? params.id : undefined;

  // The title and the stream are fetched independently on purpose: the title is usually
  // already cached, finding a stream searches torrent indexers and can take far longer.
  const detail = useDetail(key);
  // Read once per episode: the position it was left at.
  const startAt = useMemo(() => (id ? resumeAt(id) : null), [id]);
  const stream = usePlay(id, startAt);
  const [playbackError, setPlaybackError] = useState<{ id: string; message: string } | null>(null);

  const data = detail.data;
  const card = data?.media;
  const next = neighbour(data, id, 1);
  const previous = neighbour(data, id, -1);

  const leave = useCallback(() => router.push(key ? titlePath(key) : ROUTES.HOME), [key, router]);
  // Swapping episodes replaces the page, so Back still leaves the show.
  const go = useCallback(
    (episode: string) => key && router.replace(watchPath(key, episode)),
    [key, router],
  );

  const onProgress = useCallback(
    (position: number, duration: number) => {
      if (!card || !id) return;
      // Within seconds of the end the next episode is what to carry on with.
      record({ media: card, episodeId: id, position, duration });
    },
    [card, id],
  );

  const onNext = useCallback(() => {
    if (card && id && next) record({ media: card, episodeId: next, position: 0, duration: 0 });
    if (next) go(next);
  }, [card, go, id, next]);

  const link = stream.data;
  const error = playbackError && playbackError.id === id ? playbackError.message : null;

  const status = (children: React.ReactNode) => (
    <div className="fixed inset-0 z-[60] flex flex-col items-center justify-center gap-3 bg-black px-6 text-center">
      <button
        type="button"
        onClick={leave}
        className="absolute left-5 top-4 cursor-pointer text-sm font-semibold text-dim transition-colors hover:text-ink md:left-10"
      >
        ← Back
      </button>
      {children}
    </div>
  );

  if (stream.isLoading || stream.isIdle) {
    return status(
      <>
        <Spinner />
        <p className="text-[15px] font-semibold text-ink">Finding the best source</p>
        <p className="text-[13px] text-dim">This can take up to a minute.</p>
      </>,
    );
  }

  const neighbours = (
    <div className="flex gap-3 pt-3">
      {previous && (
        <Button look="secondary" onClick={() => go(previous)}>
          Previous episode
        </Button>
      )}
      {next && (
        <Button look="secondary" onClick={() => go(next)}>
          Next episode
        </Button>
      )}
    </div>
  );

  if (stream.isError) {
    return status(
      <>
        <ErrorState
          title="Couldn't load this"
          detail={playErrorMessage(stream.error)}
          onRetry={() => stream.refetch()}
        />
        {neighbours}
      </>,
    );
  }

  if (!link?.available) {
    return status(
      <>
        <ErrorState
          title="No playable source found"
          detail={link?.reason || "The API returned no stream for this title."}
          onRetry={() => stream.refetch()}
        />
        {neighbours}
      </>,
    );
  }

  const isFilm = parseMediaId(key)?.contentType === "movie";
  const count = episodeCount(data);

  return (
    <>
      <Player
        key={id}
        src={link.url}
        streamType={link.stream_type}
        subtitles={link.subtitles}
        title={titleLine(data, id)}
        startAt={startAt}
        onBack={leave}
        onError={(message) => id && setPlaybackError({ id, message })}
        onProgress={onProgress}
        onNext={next ? onNext : undefined}
        episodes={
          !isFilm && key && data ? (
            <Episodes titleKey={key} count={count} list={data.episodes} currentId={id} compact />
          ) : undefined
        }
      />
      {error && (
        <p className="fixed inset-x-0 bottom-28 z-[70] mx-auto w-fit max-w-[90vw] rounded-xl border border-line bg-surface/95 px-4 py-2 text-sm text-dim">
          {error}
        </p>
      )}
    </>
  );
}

