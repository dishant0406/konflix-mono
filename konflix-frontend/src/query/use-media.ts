"use client";

import { useQuery } from "react-query";

import { request } from "@/lib/api";
import { GET_MEDIA_DETAIL, GET_MEDIA_LIST, GET_MEDIA_SEARCH, GET_WATCH_METADATA } from "@/lib/queries";
import { QUERY_KEYS } from "./keys";
import type { MediaFull, MediaPage } from "@/types/media";

export interface MediaListFilters {
  sort?: string;
  genre?: string;
  genres?: string[];
  format?: string;
  status?: string;
  season?: string;
  year?: number;
  page?: number;
  perPage?: number;
}

/**
 * A page of media. Used by the home rows and by the search page when there is no
 * term, which is how browsing by genre, season and year works.
 */
export function useMediaList(filters: MediaListFilters, enabled = true) {
  return useQuery(
    [QUERY_KEYS.MEDIA_LIST, filters],
    () => request<{ mediaList: MediaPage }>(GET_MEDIA_LIST, { input: filters }).then((data) => data.mediaList),
    { enabled, keepPreviousData: true },
  );
}

/** A page of search results. Skipped entirely when the term is empty. */
export function useMediaSearch(term: string, filters: MediaListFilters) {
  return useQuery(
    [QUERY_KEYS.MEDIA_SEARCH, term, filters],
    () =>
      request<{ mediaSearch: MediaPage }>(GET_MEDIA_SEARCH, { term, input: filters }).then(
        (data) => data.mediaSearch,
      ),
    { enabled: term.trim().length > 0, keepPreviousData: true },
  );
}

export function useMediaDetail(id: string | undefined) {
  return useQuery(
    [QUERY_KEYS.MEDIA_DETAIL, id],
    () => request<{ mediaDetail: MediaFull | null }>(GET_MEDIA_DETAIL, { id }).then((data) => data.mediaDetail),
    { enabled: Boolean(id) },
  );
}

export function useWatchMetadata(id: string | undefined) {
  return useQuery(
    [QUERY_KEYS.WATCH_METADATA, id],
    () =>
      request<{ mediaDetail: MediaFull | null }>(GET_WATCH_METADATA, { id }).then(
        (data) => data.mediaDetail,
      ),
    { enabled: Boolean(id) },
  );
}
