"use client";

import { useSearchParams } from "next/navigation";
import { useMemo } from "react";

import type { MediaListFilters } from "@/query/use-media";

export interface SearchPageParams extends MediaListFilters {
  q: string;
}

/**
 * The search page's state lives in the URL, so a filtered search can be linked,
 * shared and reloaded. This reads it back into the shape the API expects.
 */
export function useSearchPageParams(): SearchPageParams {
  const searchParams = useSearchParams();

  return useMemo(() => {
    const get = (key: string) => searchParams.get(key) || undefined;
    const genres = get("genres");
    const year = get("year");
    return {
      q: searchParams.get("q") ?? "",
      page: Number(searchParams.get("page")) || 1,
      sort: get("sort"),
      format: get("format"),
      status: get("status"),
      season: get("season"),
      year: year ? Number(year) : undefined,
      genres: genres ? genres.split(",").filter(Boolean) : undefined,
    };
  }, [searchParams]);
}
