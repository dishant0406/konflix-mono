"use client";

import { groupReleases, playingRelease, releaseFacts } from "@/lib/releases";
import { cn } from "@/lib/utils";
import { useReleases } from "@/query/use-releases";
import type { Release } from "@/types/media";

import { Button } from "./button";
import { Skeleton, StateCard } from "./states";

const INSTANT_HINT =
  "Your debrid service already holds this release, so it starts at once. The others wait for a swarm of peers.";

/**
 * The body of the source panel: every release found for an episode or film, to play one of them
 * instead of the one chosen for you. Mounted only while the panel is open, so the search (about ten
 * seconds the first time) is not paid for by a viewer who never looks.
 *
 * `picked` is the release the viewer chose, `playedTitle` the one the server reported playing;
 * either marks a row.
 */
export default function Sources({
  id,
  picked,
  playedTitle,
  onPick,
}: {
  id: string | undefined;
  picked: string | null;
  playedTitle: string | null;
  onPick: (release: Release) => void;
}) {
  const query = useReleases(id, true);

  if (query.isLoading) {
    return (
      <div className="flex flex-col gap-1 p-2" aria-busy>
        {Array.from({ length: 6 }, (_, index) => (
          <div key={index} className="flex flex-col gap-2 rounded-lg px-3 py-3">
            <Skeleton className="h-4 w-11/12 bg-wash" />
            <Skeleton className="h-3 w-2/3 bg-wash" />
          </div>
        ))}
      </div>
    );
  }

  if (query.isError) {
    return (
      <Note title="Couldn't list the sources" detail={query.error instanceof Error ? query.error.message : "The API did not respond."}>
        <Button look="secondary" onClick={() => void query.refetch()}>
          Try again
        </Button>
      </Note>
    );
  }

  const releases = query.data ?? [];
  if (releases.length === 0) {
    return <Note title="No sources yet" detail="Nothing is listed for this one. Check back later; new releases appear as they are shared." />;
  }

  const { common, other } = groupReleases(releases);
  const playing = playingRelease(releases, picked, playedTitle);
  const row = (release: Release) => <Row key={release.id} release={release} playing={release.id === playing} onPick={onPick} />;

  return (
    <div className="flex flex-col gap-1 p-2">
      {common.map(row)}
      {other.length > 0 && (
        <>
          <p className="caps px-3 pb-2 pt-6 text-faint">Other formats · may not play in every browser</p>
          {other.map(row)}
        </>
      )}
    </div>
  );
}

/** A release is named "The.Movie.1994.1080p.BluRay.x264-GROUP": dots stand in for spaces, so show spaces. */
function readable(name: string): string {
  return !name.includes(" ") && (name.match(/\./g)?.length ?? 0) >= 3 ? name.replace(/\./g, " ") : name;
}

function Row({ release, playing, onPick }: { release: Release; playing: boolean; onPick: (release: Release) => void }) {
  return (
    <button
      type="button"
      onClick={() => onPick(release)}
      aria-current={playing ? "true" : undefined}
      className={cn(
        "relative flex w-full cursor-pointer items-start rounded-md px-3 py-3 text-left transition-colors duration-150 hover:bg-wash",
        playing && "bg-wash",
      )}
    >
      {playing && <span aria-hidden className="absolute inset-y-3 left-0 w-0.5 rounded-full bg-accent" />}
      <span className="flex min-w-0 flex-1 flex-col gap-1.5">
        <span className="line-clamp-2 break-words text-[13px] font-medium leading-snug text-ink">{readable(release.title)}</span>
        <span className="flex flex-wrap items-center gap-x-2.5 text-xs text-dim">
          {playing && <span className="caps text-accent">Playing</span>}
          {release.cached === true && (
            <span title={INSTANT_HINT} className="caps text-accent">
              Instant
            </span>
          )}
          <span>{releaseFacts(release).join(" · ")}</span>
        </span>
      </span>
    </button>
  );
}

function Note({ title, detail, children }: { title: string; detail: string; children?: React.ReactNode }) {
  return (
    <StateCard title={title} detail={detail} compact>
      {children}
    </StateCard>
  );
}
