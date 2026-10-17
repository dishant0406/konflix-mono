"use client";

import { useSearchParams } from "next/navigation";

import HomeView from "./home-view";
import SearchView from "./search-view";

/**
 * The music tab: the home shelves, or the results of a search. Searching is done from the same
 * panel as everywhere else (Cmd-K, or the magnifier in the bar), which lands here with `?q=`.
 */
export default function MusicPage() {
  const term = useSearchParams().get("q")?.trim() ?? "";
  return <div className="flex flex-col gap-8 pb-8 pt-20">{term ? <SearchView term={term} /> : <HomeView />}</div>;
}
