"use client";

import { useQuery } from "react-query";

import { request } from "@/lib/api";
import { GET_MEDIA_SEARCH } from "@/lib/queries";
import { QUERY_KEYS } from "./keys";
import type { MediaPage } from "@/types/media";

/** A few matches for the search box dropdown. */
export function useSearchSuggestions(term: string) {
  return useQuery(
    [QUERY_KEYS.MEDIA_SEARCH, "suggest", term],
    () =>
      request<{ mediaSearch: MediaPage }>(GET_MEDIA_SEARCH, {
        term,
        input: { perPage: 6, sort: "popularity" },
      }).then((data) => data.mediaSearch.results ?? []),
    { enabled: term.trim().length > 0 },
  );
}
