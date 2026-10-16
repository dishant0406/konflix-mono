"use client";

import { useMemo } from "react";
import { useInfiniteQuery, useQuery } from "react-query";

import { DEFAULT_SORT, type ContentType } from "@/constants/site";
import { ApiError, get } from "@/lib/api";
import type { Card } from "@/lib/media";
import type { Detail, HomeResponse, Media, Page } from "@/types/media";
import { QUERY_KEYS } from "./keys";

const MINUTE = 60 * 1000;
const NONE: Media[] = [];

/**
 * The home page: the banner's titles and every row, in one request. A row whose provider
 * failed comes back empty and is left out rather than blanking the page.
 */
export function useHome(initial?: HomeResponse) {
  const query = useQuery([QUERY_KEYS.HOME], () => get<HomeResponse>("/v1/home"), {
    staleTime: 5 * MINUTE,
    // What the server rendered the page with, so nothing is asked for again at once.
    initialData: initial,
  });
  const rows = useMemo(() => (query.data?.rows ?? []).filter((row) => row.items.length > 0), [query.data]);
  return {
    featured: query.data?.featured ?? NONE,
    rows,
    loading: query.isLoading,
  };
}

/**
 * A catalogue, or all three, a page at a time. With no kind the server asks the three together
 * and interleaves them, so the grid is a mix rather than all anime first.
 */
export function useBrowse(kind: ContentType | null, sort: string, genre: string | undefined, initial?: Page) {
  const query = useInfiniteQuery(
    [QUERY_KEYS.CATALOGUE, "browse", kind, sort, genre],
    ({ pageParam = 1 }) => get<Page>("/v1/catalogue", { kind: kind ?? "all", sort, genre, page: pageParam }),
    {
      getNextPageParam: (last, pages) => (last.has_next ? pages.length + 1 : undefined),
      keepPreviousData: true,
      staleTime: 5 * MINUTE,
      // The first page, as the server rendered it, for the view the page opens on.
      initialData: initial && sort === DEFAULT_SORT && !genre ? { pages: [initial], pageParams: [undefined] } : undefined,
    },
  );

  // Providers can repeat a title across pages, and a duplicate tile is a visible bug.
  const results = useMemo(() => {
    const seen = new Set<string>();
    const merged: Card[] = [];
    for (const page of query.data?.pages ?? []) {
      for (const card of page.results) {
        if (!seen.has(card.id)) {
          seen.add(card.id);
          merged.push(card);
        }
      }
    }
    return merged;
  }, [query.data]);

  return {
    results,
    loading: query.isLoading,
    failed: query.isError && results.length === 0,
    hasMore: Boolean(query.hasNextPage),
    loadingMore: query.isFetchingNextPage,
    loadMore: () => void query.fetchNextPage(),
    retry: () => void query.refetch(),
  };
}

/** Matches for a term from every catalogue, merged by the server: nobody knows which provider holds a title. */
export function useSearch(term: string, limit: number) {
  const trimmed = term.trim();
  const query = useQuery(
    [QUERY_KEYS.SEARCH, trimmed, limit],
    () => get<{ results: Media[] }>("/v1/search", { q: trimmed, limit }).then((data) => data.results),
    // The last term's matches stay up while the next term's come, so the list does not go blank on
    // every keystroke. `stale` says that is what is showing.
    { enabled: trimmed.length > 0, staleTime: 5 * MINUTE, retry: false, keepPreviousData: true },
  );
  return {
    results: trimmed ? (query.data ?? NONE) : NONE,
    stale: query.isPreviousData,
    searching: query.isFetching,
    failed: trimmed.length > 0 && query.isError,
  };
}

/** The query for a title's full record, shared by the page and by the hover prefetch. */
export function detailQueryOptions(key: string | undefined, initial?: Detail) {
  return {
    initialData: initial,
    queryKey: [QUERY_KEYS.DETAIL, key],
    queryFn: () => get<Detail>(`/v1/titles/${encodeURIComponent(key ?? "")}`),
    enabled: Boolean(key),
    staleTime: 10 * MINUTE,
    // An unknown id answers 404 every time, so asking again only delays "not found".
    retry: (count: number, error: unknown) => count < 3 && !(error instanceof ApiError && error.status === 404),
  };
}

/** A title in full. The id names the provider, so the server knows which catalogue to ask. */
export function useDetail(key: string | undefined, initial?: Detail) {
  return useQuery(detailQueryOptions(key, initial));
}

/** More of what a title is: popular in its first genre, so every page has something below it. */
export function useMoreLike(kind: ContentType | undefined, genre: string | undefined) {
  const query = useQuery(
    [QUERY_KEYS.CATALOGUE, kind, "popularity", genre],
    () => get<Page>("/v1/catalogue", { kind, sort: "popularity", genre }),
    { enabled: Boolean(kind && genre), staleTime: 5 * MINUTE },
  );
  return useMemo(() => (query.data?.results ?? NONE).slice(0, 20), [query.data]);
}
