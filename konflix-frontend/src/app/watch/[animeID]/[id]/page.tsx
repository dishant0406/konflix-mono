"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

import Loading from "@/app/loading";
import { ROUTES } from "@/constants/routes";

/**
 * The watch page used to live at /watch/<animeId>/<episodeId>. Those links are in
 * browser histories and shared messages, so the old path forwards to the new one
 * rather than 404ing.
 */
export default function LegacyWatchRedirect() {
  const params = useParams();
  const router = useRouter();

  useEffect(() => {
    const animeId = typeof params.animeID === "string" ? params.animeID : "";
    const episodeId = typeof params.id === "string" ? params.id : "";
    if (animeId && episodeId) {
      router.replace(`${ROUTES.WATCH}/${animeId}/${episodeId}`);
    } else {
      router.replace(ROUTES.HOME);
    }
  }, [params.animeID, params.id, router]);

  return <Loading />;
}
