"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

import Loading from "@/app/loading";
import { ROUTES, titlePath } from "@/constants/routes";

/**
 * The detail page used to live at /anime/<id>, and before that at /info/<id>.
 * Those links are in browser histories and search results, so the old path
 * forwards rather than 404ing.
 *
 * The bare numeric id is upgraded to the prefixed form, which is what the API
 * needs to know which provider issued it.
 */
export default function LegacyAnimeRedirect() {
  const params = useParams();
  const router = useRouter();

  useEffect(() => {
    const id = typeof params.id === "string" ? params.id : "";
    router.replace(id ? titlePath(id.includes("-") ? id : `anime-${id}`) : ROUTES.HOME);
  }, [params.id, router]);

  return <Loading />;
}
