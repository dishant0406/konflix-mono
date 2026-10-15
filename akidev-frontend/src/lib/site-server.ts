import { headers } from "next/headers";

import { PICKER_HEADER, siteForHost, type SiteConfig } from "@/constants/site";

/**
 * The site for the current request, or null on the service picker.
 *
 * Server components only. The picker host belongs to no site, and the middleware marks
 * its rewrite so the picker stays distinguishable after the rewrite has replaced the
 * host the render sees.
 */
export async function siteFromRequest(): Promise<SiteConfig | null> {
  const requestHeaders = await headers();
  if (requestHeaders.get(PICKER_HEADER)) return null;
  return siteForHost(requestHeaders.get("host"));
}
