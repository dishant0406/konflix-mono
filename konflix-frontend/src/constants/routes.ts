// Route paths.
//
// Content-agnostic on purpose: the same codebase serves an anime site and a
// films/series site, so no route is named after a content type. Which catalogue a
// page shows comes from the id it is given, not from the path.

export const ROUTES = {
  HOME: "/",
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
