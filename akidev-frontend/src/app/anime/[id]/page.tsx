import { permanentRedirect } from "next/navigation";

import { ROUTES, titlePath } from "@/constants/routes";

/**
 * The detail page used to live at /anime/<id>, and before that at /info/<id>. Those links are in
 * browser histories and search results, so the old path forwards rather than 404ing, and does it
 * on the server so the visitor never sees a page in between.
 *
 * The bare numeric id is upgraded to the prefixed form, which is what the API needs to know which
 * provider issued it.
 */
export default async function LegacyAnimeRedirect({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  permanentRedirect(id ? titlePath(id.includes("-") ? id : `anime-${id}`) : ROUTES.HOME);
}
