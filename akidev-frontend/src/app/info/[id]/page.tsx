import { permanentRedirect } from "next/navigation";

import { ROUTES, titlePath } from "@/constants/routes";

/** The oldest detail path, forwarded to the current one. See /anime/[id]. */
export default async function LegacyInfoRedirect({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  permanentRedirect(id ? titlePath(id.includes("-") ? id : `anime-${id}`) : ROUTES.HOME);
}
