"use client";

import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useEffect, useState } from "react";

import Loading from "@/app/loading";
import AnimeCard from "@/components/anime-card";
import Container from "@/components/container";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ROUTES, titlePath } from "@/constants/routes";
import { SITE } from "@/constants/site";
import { useSearchPageParams } from "@/hooks/use-search-params";
import { cn } from "@/lib/utils";
import { useMediaList, useMediaSearch } from "@/query/use-media";

// The season and year filters are AniList concepts. They are declared here rather than
// in the site config because only this page offers them, and they are only rendered
// where they do something (see SITE.supportsAdvancedFilters).
const SEASONS = [
  { value: "winter", label: "Winter" },
  { value: "spring", label: "Spring" },
  { value: "summer", label: "Summer" },
  { value: "fall", label: "Fall" },
];

const CURRENT_YEAR = new Date().getFullYear();
const YEARS = Array.from({ length: 45 }, (_, index) => CURRENT_YEAR - index);

export default function SearchPage() {
  return (
    <Suspense fallback={<Loading />}>
      <SearchResults />
    </Suspense>
  );
}

function SearchResults() {
  const params = useSearchPageParams();
  const router = useRouter();
  const searchParams = useSearchParams();

  // The panel keeps its own copy so filters can be changed and then applied,
  // which is what the Apply button implies.
  const [draft, setDraft] = useState(params);

  // The URL is the source of truth, so Back and Forward must update the panel too.
  useEffect(() => {
    setDraft(params);
  }, [params]);

  const term = params.q.trim();
  const hasTerm = term.length > 0;

  const listQuery = useMediaList(
    {
      sort: params.sort,
      genres: params.genres,
      format: params.format,
      status: params.status,
      season: params.season,
      year: params.year,
      page: params.page,
      perPage: 24,
    },
    !hasTerm,
  );
  const searchQuery = useMediaSearch(term, {
    sort: params.sort,
    genres: params.genres,
    format: params.format,
    status: params.status,
    season: params.season,
    year: params.year,
    page: params.page,
    perPage: 24,
  });

  const active = hasTerm ? searchQuery : listQuery;
  const results = active.data?.results ?? [];
  const page = params.page ?? 1;
  const hasNextPage = active.data?.hasNextPage ?? false;

  const apply = (next: Partial<typeof draft>) => {
    const merged = { ...draft, ...next };
    setDraft(merged);

    const query = new URLSearchParams();
    if (merged.q.trim()) query.set("q", merged.q.trim());
    if (merged.sort) query.set("sort", merged.sort);
    if (merged.format) query.set("format", merged.format);
    if (merged.status) query.set("status", merged.status);
    if (merged.season) query.set("season", merged.season);
    if (merged.year) query.set("year", String(merged.year));
    if (merged.genres?.length) query.set("genres", merged.genres.join(","));
    router.push(`${ROUTES.SEARCH}?${query.toString()}`);
  };

  const goToPage = (nextPage: number) => {
    const query = new URLSearchParams(searchParams.toString());
    query.set("page", String(nextPage));
    router.push(`${ROUTES.SEARCH}?${query.toString()}`);
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <Container className="flex flex-col gap-8 py-8">
      <div className="flex flex-col gap-5 rounded-lg border border-white/10 bg-white/5 p-5">
        <div className="flex items-center justify-between">
          <h1 className="text-lg font-semibold text-white">Filters</h1>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setDraft({ q: "", page: 1 });
              router.push(ROUTES.SEARCH);
            }}
          >
            Reset
          </Button>
        </div>

        <div className="flex flex-wrap gap-3">
          <FilterSelect
            label="Sort"
            value={draft.sort}
            options={SITE.sorts}
            onChange={(value) => apply({ sort: value })}
          />
          {SITE.supportsAdvancedFilters && (
            <>
              <FilterSelect
                label="Type"
                value={draft.format}
                options={SITE.formats}
                onChange={(value) => apply({ format: value })}
              />
              <FilterSelect
                label="Status"
                value={draft.status}
                options={SITE.statuses}
                onChange={(value) => apply({ status: value })}
              />
            </>
          )}
          {SITE.supportsAdvancedFilters && (
            <>
              <FilterSelect
                label="Season"
                value={draft.season}
                options={SEASONS}
                onChange={(value) => apply({ season: value })}
              />
              <Select
                value={draft.year ? String(draft.year) : undefined}
                onValueChange={(value) => apply({ year: Number(value) })}
              >
                <SelectTrigger className="w-[10rem]" aria-label="Year">
                  <SelectValue placeholder="Year" />
                </SelectTrigger>
                <SelectContent className="z-[120]">
                  {YEARS.map((year) => (
                    <SelectItem key={year} value={String(year)}>
                      {year}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </>
          )}
        </div>

        <div>
          <p className="mb-3 text-sm font-semibold text-white">Genres</p>
          <ToggleGroup
            type="multiple"
            className="flex flex-wrap justify-start gap-2"
            value={draft.genres ?? []}
            onValueChange={(value) => apply({ genres: value.filter(Boolean) })}
          >
            {SITE.genres.map((genre) => (
              <ToggleGroupItem
                key={genre}
                value={genre}
                size="sm"
                className={cn(
                  "border border-slate-700 text-slate-300 hover:border-[#e9376b] hover:bg-transparent hover:text-[#e9376b]",
                  "data-[state=on]:border-[#e9376b] data-[state=on]:bg-[#e9376b] data-[state=on]:text-white",
                )}
              >
                {genre}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
      </div>

      <h2 className="text-xl font-semibold text-white">
        {hasTerm ? (
          <>
            Results for <span className="font-extrabold">&ldquo;{term}&rdquo;</span>
          </>
        ) : (
          "Browse"
        )}
      </h2>

      {active.isLoading ? (
        <div className="grid w-full grid-cols-2 content-center gap-5 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 2xl:grid-cols-7">
          {Array.from({ length: 14 }).map((_, index) => (
            <div
              key={index}
              className="h-[15.625rem] min-w-[10.625rem] max-w-[12.625rem] animate-pulse rounded-xl bg-slate-800 md:h-[18.75rem] md:max-w-[12.5rem]"
            />
          ))}
        </div>
      ) : active.isError ? (
        <p className="py-16 text-center text-sm text-gray-400">
          Could not load results. The Konflix API may be unreachable.
        </p>
      ) : results.length === 0 ? (
        <p className="py-16 text-center text-sm text-gray-400">
          {hasTerm
            ? `Nothing matched “${term}” with those filters.`
            : `No ${SITE.itemLabelPlural} matched those filters.`}
        </p>
      ) : (
        <div className="grid w-full grid-cols-2 content-center gap-5 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 2xl:grid-cols-7">
          {results.map((item, index) => (
            <AnimeCard
              key={`${item.id}-${index}`}
              media={item}
              href={titlePath(item.id)}
              className="self-center justify-self-center"
            />
          ))}
        </div>
      )}

      <div className="flex items-center justify-center gap-3">
        <Button variant="secondary" disabled={page <= 1} onClick={() => goToPage(page - 1)}>
          Previous
        </Button>
        <span className="text-sm text-gray-400">Page {page}</span>
        <Button variant="secondary" disabled={!hasNextPage} onClick={() => goToPage(page + 1)}>
          Next
        </Button>
      </div>
    </Container>
  );
}

function FilterSelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value?: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="w-[11rem]" aria-label={label}>
        <SelectValue placeholder={label} />
      </SelectTrigger>
      <SelectContent className="z-[120]">
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
