import { NextResponse, type NextRequest } from "next/server";

import { SITE_DOMAIN } from "@/constants/site";

/**
 * Retired hostnames send visitors to the one site.
 *
 * The site used to be two, anime.akidev.live and tv.akidev.live. It is now one, with Anime,
 * Series and Films tabs, so each old host redirects to the matching page of the main one and
 * keeps the rest of the path, which keeps old title and watch links working.
 */
const RETIRED: Record<string, string> = {
  [`anime.${SITE_DOMAIN}`]: "/anime",
  [`tv.${SITE_DOMAIN}`]: "/films",
};

export function middleware(request: NextRequest) {
  const host = request.headers.get("host")?.trim().toLowerCase() ?? "";
  const landing = RETIRED[host];
  if (!landing) return NextResponse.next();

  const target = new URL(request.url);
  target.protocol = "https:";
  target.host = SITE_DOMAIN;
  target.port = "";
  if (request.nextUrl.pathname === "/") target.pathname = landing;
  return NextResponse.redirect(target, 308);
}

export const config = {
  // Everything except Next's own assets and static files.
  matcher: ["/((?!_next/|api/|favicon.ico|.*\\.).*)"],
};
