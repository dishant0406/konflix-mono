"use client";

import { useParams, useRouter } from "next/navigation";
import { useCallback, useMemo, useState } from "react";

import { Button } from "@/components/button";
import Episodes from "@/components/episodes";
import Player from "@/components/player";
import { SidePanel } from "@/components/side-panel";
import Sources from "@/components/sources";
import { ErrorState, Spinner } from "@/components/states";
import { ROUTES, titlePath, watchPath } from "@/constants/routes";
import { track } from "@/lib/analytics";
import { episodeCount, neighbour, titleLine } from "@/lib/episodes";
import { parseMediaId } from "@/lib/media-id";
import { playErrorMessage } from "@/lib/play";
import { record, resumeAt } from "@/lib/progress";
import { useDetail } from "@/query/use-media";
import { usePlay } from "@/query/use-play";
import { useSubtitles } from "@/query/use-subtitles";
import type { Release } from "@/types/media";

/** A release the viewer picked for one episode, and where they were in it. */
type Pick = { episode: string; release: Release; start: number | null };

/** Before this many seconds into an episode there is nothing to resume. */
const RESUME_AFTER = 5;

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
  // A picked release belongs to the episode it was picked for; moving to another one forgets it.
  const [picked, setPicked] = useState<Pick | null>(null);
  const pick = picked && picked.episode === id ? picked : null;
  const resumeFrom = pick ? pick.start : startAt;
  const stream = usePlay(id, resumeFrom, pick?.release.id);
  const subtitles = useSubtitles(id);
  /** The episode whose source picker is open over the "no source" screen. */
  const [choosing, setChoosing] = useState<string | null>(null);
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

  // Picking a release plays that one, from where the viewer was.
  const choose = useCallback(
    (release: Release, position: number) => {
      if (!id) return;
      setChoosing(null);
      setPicked({ episode: id, release, start: position >= RESUME_AFTER ? Math.floor(position) : null });
    },
    [id],
  );

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
      {choosing === id && (
        <SidePanel title="Sources" onClose={() => setChoosing(null)} className="sm:bottom-4 sm:top-14">
          <Sources id={id} picked={null} playedTitle={null} onPick={(release) => choose(release, 0)} />
        </SidePanel>
      )}
    </div>
  );

  if (stream.isLoading || stream.isIdle) {
    return status(
      <>
        <Spinner />
        <p className="text-[15px] font-semibold text-ink">{pick ? "Finding this source" : "Finding the best source"}</p>
        <p className="text-[13px] text-dim">This can take up to a minute.</p>
      </>,
    );
  }

  const neighbours = (
    <div className="flex flex-wrap justify-center gap-3 pt-3">
      <Button look="secondary" onClick={() => setChoosing(id ?? null)}>
        Choose another source
      </Button>
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
        subtitles={subtitles.subtitles}
        subtitlesLoading={subtitles.loading}
        title={titleLine(data, id)}
        startAt={resumeFrom}
        onBack={leave}
        onError={(message) => {
          track("playback_failed", { title_key: key ?? null, stream_type: link.stream_type, message });
          if (id) setPlaybackError({ id, message });
        }}
        onStarted={() => track("playback_started", { title_key: key ?? null, stream_type: link.stream_type })}
        onProgress={onProgress}
        onNext={next ? onNext : undefined}
        sources={({ position, close }) => (
          <Sources
            id={id}
            picked={pick?.release.id ?? null}
            playedTitle={link.release}
            onPick={(release) => {
              close();
              // The one already playing is not started over.
              if (release.id !== pick?.release.id && (pick || release.title !== link.release)) choose(release, position());
            }}
          />
        )}
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

