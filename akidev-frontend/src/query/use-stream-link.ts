"use client";

import { useQuery } from "react-query";

import { request } from "@/lib/api";
import { GET_STREAM_LINK } from "@/lib/queries";
import { QUERY_KEYS } from "./keys";
import type { StreamLink } from "@/types/media";

/**
 * Resolves one episode to a playable source.
 *
 * This is the slow call - it searches torrent indexers and may join a swarm - so
 * it is deliberately separate from the metadata query. The page renders the title
 * and episode list from metadata while this is still in flight.
 *
 * A result with `available: false` is a normal outcome, not an error, so it is
 * cached like any other and rendered as an honest message.
 */
export function useStreamLink(episodeId: string | undefined) {
  return useQuery(
    [QUERY_KEYS.STREAM_LINK, episodeId],
    () =>
      request<{ streamLinkDetails: StreamLink }>(GET_STREAM_LINK, { id: episodeId }).then(
        (data) => data.streamLinkDetails,
      ),
    {
      enabled: Boolean(episodeId),
      // Resolving is expensive on the server, so a resolved episode is kept for a
      // while and re-used when the viewer moves back and forth.
      staleTime: 10 * 60 * 1000,
      retry: false,
    },
  );
}
