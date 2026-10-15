import { permanentRedirect } from "next/navigation";

import { ROUTES, watchPath } from "@/constants/routes";

/**
 * The watch page used to live at /anime/watch/<id>/<episodeId>. Those links are in browser
 * histories and shared messages, so the old path forwards rather than 404s, on the server.
 *
 * Both segments are upgraded to the prefixed id form, which is what the API needs to select a
 * provider.
 */
export default async function LegacyWatchRedirect({ params }: { params: Promise<{ animeID: string; id: string }> }) {
  const { animeID, id } = await params;
  if (!animeID || !id) permanentRedirect(ROUTES.HOME);
  const key = animeID.includes("-") ? animeID : `anime-${animeID}`;
  const media = id.includes("-") ? id : `anime-${animeID}-ep${id}`;
  permanentRedirect(watchPath(key, media));
}
