"use client";

import { ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";

import { CATALOGUES, SORTS, genresFor, type ContentType } from "@/constants/site";
import type { Card } from "@/lib/media";
import { cn } from "@/lib/utils";
import { useBrowse, useSearch } from "@/query/use-media";

import { Button, Tab } from "./button";
import { EmptyState, ErrorState, Skeleton } from "./states";
import Tile from "./tile";

const SKELETONS = 12;

function Grid({ cards }: { cards: Card[] }) {
  return (
    <div className="flex flex-wrap gap-x-5 gap-y-6">
      {cards.map((card, index) => (
        <Tile key={card.id} card={card} priority={index < 6} />
      ))}
    </div>
  );
}

function Loading() {
  return (
    <div className="flex flex-wrap gap-5">
      {Array.from({ length: SKELETONS }, (_, index) => (
        <Skeleton key={index} className="h-[210px] w-[140px] rounded-lg sm:h-[264px] sm:w-[176px]" />
      ))}
    </div>
  );
}

function Page({ heading, filters, children }: { heading: string; filters: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="gutter flex flex-col gap-6 pb-16 pt-[84px]">
      <h1 className="text-4xl font-black text-ink">{heading}</h1>
      {filters}
      {children}
    </div>
  );
}

/** One catalogue, or all of them: sort tabs, a genre list, and a grid that loads more. */
export function CatalogueView({ kind }: { kind: ContentType | null }) {
  const [sort, setSort] = useState(SORTS[0].value);
  const [genre, setGenre] = useState<string | undefined>();
  const [genresOpen, setGenresOpen] = useState(false);
  const browse = useBrowse(kind, sort, genre);

  const heading = CATALOGUES.find((page) => page.kind === kind)?.label ?? "Browse";

  const filters = (
    <div className="flex flex-col">
      <div className="flex items-center justify-between gap-4">
        <div className="no-scrollbar flex gap-5 overflow-x-auto">
          {SORTS.map((each) => (
            <Tab key={each.value} active={sort === each.value} onClick={() => setSort(each.value)}>
              {each.label}
            </Tab>
          ))}
        </div>
        <button
          type="button"
          onClick={() => setGenresOpen((open) => !open)}
          aria-expanded={genresOpen}
          className="flex shrink-0 cursor-pointer items-center gap-1.5 pb-[7px] text-sm"
        >
          <span className="text-dim">{genre ? "Genre" : "All genres"}</span>
          {genre && <span className="font-semibold text-ink">{genre}</span>}
          {genresOpen ? <ChevronUp className="size-4 text-dim" /> : <ChevronDown className="size-4 text-dim" />}
        </button>
      </div>
      {genresOpen && (
        <div className="flex flex-wrap gap-x-6 gap-y-2 pt-5">
          {genresFor(kind).map((each) => {
            const active = genre === each;
            return (
              <button
                key={each}
                type="button"
                onClick={() => setGenre(active ? undefined : each)}
                className={cn(
                  "flex cursor-pointer items-center gap-2 py-1 text-sm transition-colors hover:text-ink",
                  active ? "font-semibold text-ink" : "text-dim",
                )}
              >
                <span className={cn("size-[5px] rounded-full", active && "bg-accent")} />
                {each}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );

  return (
    <Page heading={heading} filters={filters}>
      {browse.loading ? (
        <Loading />
      ) : browse.failed ? (
        <ErrorState title="Couldn't reach the catalogue" detail="Check your connection and try again." onRetry={browse.retry} />
      ) : browse.results.length === 0 ? (
        <EmptyState title="No results" detail="Nothing matches these filters." />
      ) : (
        <>
          <Grid cards={browse.results} />
          {browse.hasMore && (
            <div className="flex justify-center pt-4">
              <Button look="secondary" onClick={browse.loadMore} disabled={browse.loadingMore}>
                {browse.loadingMore ? "Loading..." : "Load more"}
              </Button>
            </div>
          )}
        </>
      )}
    </Page>
  );
}

const KIND_TABS: { kind: ContentType | null; label: string }[] = [
  { kind: null, label: "All" },
  ...CATALOGUES.map((page) => ({ kind: page.kind, label: page.label })),
];

/** The full results of a search, across every catalogue, narrowed by a tab. */
export function SearchView({ term }: { term: string }) {
  const [kind, setKind] = useState<ContentType | null>(null);
  const search = useSearch(term, 60, 20);
  const shown = search.results.filter((card) => !kind || card.kind === kind);

  const filters = (
    <div className="flex gap-5">
      {KIND_TABS.map((tab) => (
        <Tab key={tab.label} active={kind === tab.kind} onClick={() => setKind(tab.kind)}>
          {tab.label}
        </Tab>
      ))}
    </div>
  );

  return (
    <Page heading={`Results for “${term}”`} filters={filters}>
      {search.searching && search.results.length === 0 ? (
        <Loading />
      ) : search.failed ? (
        <ErrorState title="Couldn't reach the catalogue" detail="Check your connection and try again." />
      ) : shown.length === 0 ? (
        <EmptyState title="No results" detail="Nothing matched. Try another spelling or fewer words." />
      ) : (
        <Grid cards={shown} />
      )}
    </Page>
  );
}
