"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

import Loading from "@/app/loading";
import { ROUTES } from "@/constants/routes";

/**
 * The detail page used to live at /info/<id>. Those links are in browser histories
 * and search results, so the old path forwards to the new one rather than 404ing.
 */
export default function LegacyInfoRedirect() {
  const params = useParams();
  const router = useRouter();

  useEffect(() => {
    const id = typeof params.id === "string" ? params.id : "";
    router.replace(id ? `${ROUTES.ANIME_DETAILS}/${id}` : ROUTES.HOME);
  }, [params.id, router]);

  return <Loading />;
}
