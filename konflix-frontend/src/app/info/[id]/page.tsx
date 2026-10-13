"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

import Loading from "@/app/loading";
import { ROUTES, titlePath } from "@/constants/routes";

/** The oldest detail path, forwarded to the current one. See /anime/[id]. */
export default function LegacyInfoRedirect() {
  const params = useParams();
  const router = useRouter();

  useEffect(() => {
    const id = typeof params.id === "string" ? params.id : "";
    router.replace(id ? titlePath(id.includes("-") ? id : `anime-${id}`) : ROUTES.HOME);
  }, [params.id, router]);

  return <Loading />;
}
