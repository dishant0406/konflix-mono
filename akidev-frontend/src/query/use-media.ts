"use client";

import { useMemo } from "react";
import { useInfiniteQuery, useQueries, useQuery } from "react-query";

import { CONTENT_TYPES, HOME_ROWS, type ContentType } from "@/constants/site";
import { request } from "@/lib/api";
import { toCards, type Card } from "@/lib/media";
import { interleaveResults, preferTitleMatches } from "@/lib/merge-search";
import { parseMediaId } from "@/lib/media-id";
import { GET_MEDIA_DETAIL, GET_MEDIA_LIST, GET_MEDIA_SEARCH, GET_WATCH_METADATA } from "@/lib/queries";
import type { MediaFull, MediaPage } from "@/types/media";
import { QUERY_KEYS } from "./keys";

/** A page of one catalogue. The content type is what selects the provider. */
function listQuery(kind: ContentType, sort: string | undefined, genre: string | undefined, perPage: number, page = 1) {
  return request<{ mediaList: MediaPage }>(GET_MEDIA_LIST, {
    input: { contentType: kind, sort, genre, perPage, page },
  }).then((data) => data.mediaList);
}

const MINUTE = 60 * 1000;

/** The home page's rows, one query each. A row that fails is left out rather than blanking the page. */
export function useHomeRows() {
  const queries = useQueries(
    HOME_ROWS.map((row) => ({
      queryKey: [QUERY_KEYS.MEDIA_LIST, row.kind, row.sort, row.genre, 24],
      queryFn: () => listQuery(row.kind, row.sort, row.genre, 24),
      staleTime: 5 * MINUTE,
    })),
  );
  return HOME_ROWS.map((row, index) => ({
    ...row,
    loading: queries[index].isLoading,
    items: toCards(queries[index].data?.results),
  }));
}

/**
 * The banner's titles: the front of each catalogue, so it features all three, and only titles
 * that have wide art.
 */
export function useFeatured() {
  const queries = useQueries(
    CONTENT_TYPES.map((kind) => ({
      queryKey: [QUERY_KEYS.MEDIA_LIST, kind, "trending", undefined, 6],
      queryFn: () => listQuery(kind, "trending", undefined, 6),
      staleTime: 5 * MINUTE,
    })),
  );
  const pages = queries.map((query) => query.data);
  const items = useMemo(
    () =>
      interleaveResults(
        pages.map((page) => page?.results ?? []),
        9,
      )
        .flatMap((media) => toCards([media]))
        .filter((card) => card.banner),
    // `pages` is a fresh array each render; its contents are what matter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [queries.map((query) => query.dataUpdatedAt).join(",")],
  );
  return { items, loading: queries.every((query) => query.isLoading) };
}

/**
 * A catalogue, or all three, a page at a time. With no kind the three are asked together and
 * interleaved, so the grid is a mix rather than all anime first.
 */
export function useBrowse(kind: ContentType | null, sort: string, genre: string | undefined) {
  const kinds = kind ? [kind] : CONTENT_TYPES;
  const query = useInfiniteQuery(
    [QUERY_KEYS.MEDIA_LIST, "browse", kind, sort, genre],
    async ({ pageParam = 1 }) => {
      const settled = await Promise.allSettled(kinds.map((each) => listQuery(each, sort, genre, 24, pageParam)));
      const answered = settled.flatMap((result) => (result.status === "fulfilled" ? [result.value] : []));
      if (answered.length === 0) throw new Error("no catalogue answered");
      return {
        results: toCards(interleaveResults(answered.map((page) => page.results ?? []), Number.MAX_SAFE_INTEGER)),
        hasNext: answered.some((page) => page.hasNextPage),
      };
    },
    {
      getNextPageParam: (last, pages) => (last.hasNext ? pages.length + 1 : undefined),
      keepPreviousData: true,
      staleTime: 5 * MINUTE,
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

/** Matches for a term from every catalogue, merged: nobody knows which provider holds a title. */
export function useSearch(term: string, limit: number, perKind = limit) {
  const trimmed = term.trim();
  const queries = useQueries(
    CONTENT_TYPES.map((kind) => ({
      queryKey: [QUERY_KEYS.MEDIA_SEARCH, kind, trimmed, perKind],
      queryFn: () =>
        request<{ mediaSearch: MediaPage }>(GET_MEDIA_SEARCH, {
          term: trimmed,
          input: { perPage: perKind, sort: "popularity", contentType: kind },
        }).then((data) => data.mediaSearch.results ?? []),
      enabled: trimmed.length > 0,
      staleTime: 5 * MINUTE,
      retry: false,
    })),
  );
  const stamp = queries.map((query) => query.dataUpdatedAt).join(",");
  const results = useMemo(
    () => toCards(preferTitleMatches(interleaveResults(queries.map((query) => query.data ?? []), limit), trimmed)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [stamp, trimmed, limit],
  );
  return {
    results,
    searching: queries.some((query) => query.isFetching),
    // One catalogue failing does not blank a search the others answered.
    failed: trimmed.length > 0 && queries.every((query) => query.isError),
  };
}

/** The query for a title's full record, shared by the page and by the hover prefetch. */
export function detailQueryOptions(key: string | undefined) {
  const contentType = parseMediaId(key)?.contentType ?? "anime";
  return {
    queryKey: [QUERY_KEYS.MEDIA_DETAIL, key],
    queryFn: () =>
      request<{ mediaDetail: MediaFull | null }>(GET_MEDIA_DETAIL, { id: key, input: { contentType } }).then(
        (data) => data.mediaDetail,
      ),
    enabled: Boolean(key),
    staleTime: 10 * MINUTE,
  };
}

/** A title's full record. The catalogue comes from the id, which names the provider. */
export function useDetail(key: string | undefined) {
  return useQuery(detailQueryOptions(key));
}

/** The watch page's metadata: the title and its episode list, without the detail extras. */
export function useWatchMetadata(key: string | undefined) {
  const contentType = parseMediaId(key)?.contentType ?? "anime";
  return useQuery(
    [QUERY_KEYS.WATCH_METADATA, key],
    () =>
      request<{ mediaDetail: MediaFull | null }>(GET_WATCH_METADATA, { id: key, input: { contentType } }).then(
        (data) => data.mediaDetail,
      ),
    { enabled: Boolean(key), staleTime: 10 * MINUTE },
  );
}

/** More of what a title is: popular in its first genre, so every page has something below it. */
export function useMoreLike(kind: ContentType | undefined, genre: string | undefined) {
  const query = useQuery(
    [QUERY_KEYS.MEDIA_LIST, kind, "popularity", genre, 20],
    () => listQuery(kind as ContentType, "popularity", genre, 20),
    { enabled: Boolean(kind && genre), staleTime: 5 * MINUTE },
  );
  return toCards(query.data?.results);
}
