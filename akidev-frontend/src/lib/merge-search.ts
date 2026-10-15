// Merging search results from several catalogues.
//
// The API's search is scoped to one content type, because a title is addressed by a
// provider-specific id. A site that carries more than one catalogue therefore has to
// search each and merge the answers, which is what this does.
//
// Kept apart from the React hook that calls it so it is a plain function over plain
// data: no React, no network, and therefore testable.

import type { MediaList, MediaPage } from "@/types/media";

/**
 * Interleaves per-catalogue result lists into one page.
 *
 * Round-robin rather than concatenation, so the first page shows a mix. Concatenating
 * would fill the whole page from whichever catalogue happens to be listed first, which
 * on the TV site means a film search returning series first.
 *
 * Ordering within each catalogue is the provider's own relevance. The merge does not
 * attempt to rank across catalogues because there is no score they share.
 */
export function interleaveResults(lists: MediaList[][], limit: number): MediaList[] {
  const usable = lists.filter((list) => list.length > 0);
  if (usable.length === 0) return [];
  if (usable.length === 1) return usable[0].slice(0, limit);

  const merged: MediaList[] = [];
  for (let index = 0; merged.length < limit; index += 1) {
    let added = false;
    for (const list of usable) {
      if (index < list.length && merged.length < limit) {
        merged.push(list[index]);
        added = true;
      }
    }
    // Every list is exhausted, so there is nothing left to add.
    if (!added) break;
  }
  return merged;
}

/** Builds the single page the results view renders from per-catalogue pages. */
export function mergeSearchPages(pages: (MediaPage | undefined)[], perPage: number): MediaPage {
  const lists = pages.map((page) => page?.results ?? []).filter((list) => list.length > 0);
  const results = interleaveResults(lists, perPage);

  return {
    page: 1,
    perPage,
    total: results.length,
    // A search spans several catalogues and is merged here, so there is no upstream
    // page to advance to. Reporting true would offer a Next button that does nothing.
    hasNextPage: false,
    results,
  };
}

// Letters and digits only, so punctuation and spacing do not decide a match.
// "Game of Thrones" and "game of thrones" normalise to the same thing.
function normalise(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function titleOf(media: MediaList): string {
  return media.title?.english || media.title?.romaji || media.title?.native || "";
}

/**
 * Moves results whose title starts with the search term to the front.
 *
 * Without this the round-robin order puts an approximate match first: TVMaze's own
 * search is fuzzy, so searching "dangal" returned "D.N.Angel" before the film "Dangal".
 * The match is a stable partition, so the interleaving is otherwise preserved and
 * nothing is dropped.
 *
 * This compares the term against a title - it does not rank one catalogue above another.
 * There is no score the providers share, so anything more would be inventing one.
 */
export function preferTitleMatches(results: MediaList[], term: string): MediaList[] {
  const wanted = normalise(term);
  if (!wanted) return results;

  const startsWithTerm: MediaList[] = [];
  const rest: MediaList[] = [];
  for (const media of results) {
    const title = normalise(titleOf(media));
    // A title that begins with the term, or one the term covers entirely. The second
    // case allows a single trailing character of slack - a term typed as "dangal" still
    // matches a title stored as "Dangal (2016)", while a four-letter title like "DANG!"
    // does not match it, which it would if the check were a plain prefix test.
    const covered = title.length > 0 && wanted.length - title.length <= 1 && wanted.startsWith(title);
    if (title.startsWith(wanted) || covered) {
      startsWithTerm.push(media);
    } else {
      rest.push(media);
    }
  }
  return [...startsWithTerm, ...rest];
}

