"use client";

import { useQuery } from "react-query";

import { get } from "@/lib/api";
import type { Release } from "@/types/media";
import { QUERY_KEYS } from "./keys";

/**
 * Every release found for an episode or film, for the source picker. The first call searches the
 * indexers and takes about ten seconds, so it is asked only while the picker is open; the server
 * keeps the answer, and so does this, so opening it again is instant.
 */
export function useReleases(id: string | undefined, enabled: boolean) {
  return useQuery(
    [QUERY_KEYS.RELEASES, id],
    () => get<{ releases: Release[] }>("/v1/releases", { id }).then((data) => data.releases),
    { enabled: enabled && Boolean(id), staleTime: 10 * 60 * 1000, retry: false },
  );
}
