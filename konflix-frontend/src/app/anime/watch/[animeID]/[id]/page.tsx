"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

import Loading from "@/app/loading";
import { ROUTES, watchPath } from "@/constants/routes";

/**
 * The watch page used to live at /anime/watch/<id>/<episodeId>. Those links are in
 * browser histories and shared messages, so the old path forwards rather than 404s.
 *
 * Both segments are upgraded to the prefixed id form, which is what the API needs
 * to select a provider.
 */
export default function LegacyWatchRedirect() {
  const params = useParams();
  const router = useRouter();

  useEffect(() => {
    const animeId = typeof params.animeID === "string" ? params.animeID : "";
    const episodeId = typeof params.id === "string" ? params.id : "";
    if (!animeId || !episodeId) {
      router.replace(ROUTES.HOME);
      return;
    }
    const key = animeId.includes("-") ? animeId : `anime-${animeId}`;
    const media = episodeId.includes("-") ? episodeId : `anime-${animeId}-ep${episodeId}`;
    router.replace(watchPath(key, media));
  }, [params.animeID, params.id, router]);

  return <Loading />;
}
