// Route paths.

export const ROUTES = {
  HOME: "/",
  /** Search results. Takes `?q=`. */
  SEARCH: "/search",
  /** A title's detail page. Takes the title's key, e.g. "/title/series-169". */
  TITLE: "/title",
  /** A watch page. Takes the title's key, then a media id, e.g.
   *  "/watch/series-169/series-169-s1e5". */
  WATCH: "/watch",
};

export function titlePath(key: string): string {
  return `${ROUTES.TITLE}/${key}`;
}

export function watchPath(key: string, mediaId: string): string {
  return `${ROUTES.WATCH}/${key}/${mediaId}`;
}

export function searchPath(term: string): string {
  return `${ROUTES.SEARCH}?q=${encodeURIComponent(term)}`;
}
