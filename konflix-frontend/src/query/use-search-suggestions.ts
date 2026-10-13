"use client";

import { useQueries } from "react-query";

import { SITE } from "@/constants/site";
import { request } from "@/lib/api";
import { GET_MEDIA_SEARCH } from "@/lib/queries";
import { interleaveResults, preferTitleMatches } from "@/lib/merge-search";
import { QUERY_KEYS } from "./keys";
import type { MediaPage } from "@/types/media";

const SUGGESTION_LIMIT = 6;

/**
 * A few matches for the search box dropdown.
 *
 * Every catalogue the site carries is searched, for the same reason the results page
 * does: the user does not know which provider holds a title. Without this the box on the
 * TV site answered from the anime catalogue, so typing a series name returned nothing.
 *
 * The per-catalogue limit is the full dropdown width rather than a share of it, so a
 * catalogue that has matches is not crowded out by one that has none.
 */
export function useSearchSuggestions(term: string) {
  const enabled = term.trim().length > 0;

  const queries = useQueries(
    SITE.searchContentTypes.map((contentType) => ({
      queryKey: [QUERY_KEYS.MEDIA_SEARCH, "suggest", contentType, term],
      queryFn: () =>
        request<{ mediaSearch: MediaPage }>(GET_MEDIA_SEARCH, {
          term,
          input: { perPage: SUGGESTION_LIMIT, sort: "popularity", contentType },
        }).then((data) => data.mediaSearch.results ?? []),
      enabled,
    })),
  );

  return {
    data: preferTitleMatches(
      interleaveResults(queries.map((query) => query.data ?? []), SUGGESTION_LIMIT),
      term,
    ),
    isFetching: queries.some((query) => query.isFetching),
  };
}
