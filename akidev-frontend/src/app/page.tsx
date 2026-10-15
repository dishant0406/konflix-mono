import HomeView from "@/components/home-view";
import { isClientNavigation, serverGet } from "@/lib/api-server";
import type { HomeResponse } from "@/types/media";

// Rendered for each request, with the home page's data already in it, so the first paint does not
// wait for the scripts to arrive and then for a request of their own. The data itself is kept for a
// minute, and the page for half a minute at the edge (see next.config.mjs).
export const dynamic = "force-dynamic";

export default async function Page() {
  const home = (await isClientNavigation()) ? null : await serverGet<HomeResponse>("/v1/home", 60);
  return <HomeView initial={home ?? undefined} />;
}
