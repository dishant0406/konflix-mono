"use client";

import { useQuery } from "react-query";

import { get } from "@/lib/api";
import type { SubtitleOption } from "@/types/media";
import { QUERY_KEYS } from "./keys";

const NONE: SubtitleOption[] = [];

/**
 * The subtitles for an episode or film. Cheap and kept by the server, so it is asked when the
 * watch page opens; it never gates playback, and a failure is the same as finding none.
 */
export function useSubtitles(id: string | undefined) {
  const query = useQuery(
    [QUERY_KEYS.SUBTITLES, id],
    () => get<{ subtitles: SubtitleOption[] }>("/v1/subtitles", { id }).then((data) => data.subtitles),
    { enabled: Boolean(id), staleTime: 30 * 60 * 1000, retry: false },
  );
  return { subtitles: query.data ?? NONE, loading: query.isLoading };
}
