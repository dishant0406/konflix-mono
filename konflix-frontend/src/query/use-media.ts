"use client";

import { useMemo } from "react";
import { useQueries, useQuery } from "react-query";

import { request } from "@/lib/api";
import { GET_MEDIA_DETAIL, GET_MEDIA_LIST, GET_MEDIA_SEARCH, GET_WATCH_METADATA } from "@/lib/queries";
import { SITE, type ContentType } from "@/constants/site";
import { parseMediaId } from "@/lib/media-id";
import { mergeSearchPages, preferTitleMatches } from "@/lib/merge-search";
import { QUERY_KEYS } from "./keys";
import type { MediaFull, MediaPage } from "@/types/media";

export interface MediaListFilters {
  /** Which catalogue to browse. Defaults to this site's own. */
  contentType?: ContentType;
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
 *
 * The content type is always sent, because it is what selects the provider. Without
 * it the API answers from the anime catalogue, which on the films site would be the
 * wrong catalogue entirely.
 */
export function useMediaList(filters: MediaListFilters, enabled = true) {
  const input = { contentType: SITE.contentType, ...filters };
  return useQuery(
    [QUERY_KEYS.MEDIA_LIST, input],
    () => request<{ mediaList: MediaPage }>(GET_MEDIA_LIST, { input }).then((data) => data.mediaList),
    { enabled, keepPreviousData: true },
  );
}

/**
 * Search results drawn from every catalogue the site carries.
 *
 * One request per catalogue, merged into a single page. The API's search is scoped to
 * one content type, because a title is addressed by a provider-specific id, so covering
 * both is a fan-out here rather than a parameter.
 *
 * Results are interleaved in proportion to how many each catalogue returned, so a site
 * whose series outnumber its films does not have its first page filled entirely by
 * series. Ordering inside each catalogue is the provider's own relevance, which is why
 * the merge does not attempt to rank across them - there is no shared score to rank by.
 */
export function useMediaSearchAcross(term: string, filters: MediaListFilters = {}) {
  const types = SITE.searchContentTypes;
  const perPage = filters.perPage ?? 24;

  const queries = useQueries(
    types.map((contentType) => ({
      queryKey: [QUERY_KEYS.MEDIA_SEARCH, term, contentType, filters],
      queryFn: () =>
        request<{ mediaSearch: MediaPage }>(GET_MEDIA_SEARCH, {
          term,
          // The per-catalogue page is the full requested width, so a catalogue that has
          // matches is never starved by one that has none.
          input: { ...filters, perPage, contentType },
        }).then((data) => data.mediaSearch),
      enabled: term.trim().length > 0,
      keepPreviousData: true,
    })),
  );

  const pages = queries.map((query) => query.data);
  const merged = useMemo(() => {
    const page = mergeSearchPages(pages, perPage);
    return { ...page, results: preferTitleMatches(page.results ?? [], term) };
  }, [pages, perPage, term]);

  return {
    data: merged,
    // A single catalogue failing should not blank a search that the others answered.
    isLoading: queries.every((query) => query.isLoading),
    isError: queries.every((query) => query.isError),
    isFetching: queries.some((query) => query.isFetching),
  };
}

/**
 * A title's full record.
 *
 * The content type comes from the id rather than from the site, because a detail page
 * is addressed by a self-describing id and the API needs to know which provider issued
 * it. A bare id with no type falls back to this site's catalogue.
 */
export function useMediaDetail(id: string | undefined) {
  const contentType = parseMediaId(id)?.contentType ?? SITE.contentType;
  return useQuery(
    [QUERY_KEYS.MEDIA_DETAIL, id],
    () =>
      request<{ mediaDetail: MediaFull | null }>(GET_MEDIA_DETAIL, {
        id,
        input: { contentType },
      }).then((data) => data.mediaDetail),
    { enabled: Boolean(id) },
  );
}

/** The watch page's metadata: the title and its episode list, without the detail extras. */
export function useWatchMetadata(id: string | undefined) {
  const contentType = parseMediaId(id)?.contentType ?? SITE.contentType;
  return useQuery(
    [QUERY_KEYS.WATCH_METADATA, id],
    () =>
      request<{ mediaDetail: MediaFull | null }>(GET_WATCH_METADATA, {
        id,
        input: { contentType },
      }).then((data) => data.mediaDetail),
    { enabled: Boolean(id) },
  );
}

export interface MediaRow {
  title: string;
  sort?: string;
  genre?: string;
  /** Which catalogue this row draws from. Defaults to the site's own. */
  contentType?: ContentType;
}

/**
 * Several rows at once, one query per row.
 *
 * `useQueries` rather than a loop of `useMediaList` calls: the number of rows comes
 * from the site config, and calling a hook inside a `.map` is a rules-of-hooks
 * violation that React cannot guarantee. This is the API designed for a dynamic list
 * of queries.
 */
export function useMediaRows(rows: MediaRow[], perPage: number) {
  return useQueries(
    rows.map((row) => {
      const contentType = row.contentType ?? SITE.contentType;
      return {
        queryKey: [QUERY_KEYS.MEDIA_LIST, contentType, row.sort, row.genre, perPage],
        queryFn: () =>
          request<{ mediaList: MediaPage }>(GET_MEDIA_LIST, {
            input: { contentType, sort: row.sort, genre: row.genre, perPage },
          }).then((data) => data.mediaList),
        keepPreviousData: true,
      };
    }),
  );
}
