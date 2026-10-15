import { NextResponse, type NextRequest } from "next/server";

import { PICKER_HEADER, siteForHost } from "@/constants/site";

/**
 * Keeps each hostname to its own job.
 *
 * A host that belongs to a site serves the app unchanged. The bare zone belongs to no
 * site, so every path there is rewritten to the picker - the app's routes need a site to
 * render, and without one they would render with no copy and no catalogue.
 *
 * A rewrite rather than a redirect, so the address bar keeps the zone's own root while
 * the picker is what actually renders.
 */
export function middleware(request: NextRequest) {
  // A rewrite re-enters this function, and by then the URL is the rewritten one - the
  // host is the server's own address, which maps to a site. The header set on the first
  // pass is what stops the second pass from misreading the picker as a site request and
  // bouncing it in a loop.
  if (request.headers.get(PICKER_HEADER)) {
    return NextResponse.next();
  }

  const site = siteForHost(request.headers.get("host"));

  if (site) {
    // The picker offers a choice that a visitor on a site host has already made, and it
    // would render inside that site's chrome. Send them to the site's own home instead
    // of rendering it, so the redirect is a real one rather than a client-side hop.
    if (request.nextUrl.pathname === "/picker") {
      return NextResponse.redirect(new URL("/", request.url), 308);
    }
    return NextResponse.next();
  }

  const url = request.nextUrl.clone();
  url.pathname = "/picker";

  // The rewrite replaces the URL the render sees, so the host no longer identifies the
  // picker. Marking it here is what lets the layout tell the two apart.
  const requestHeaders = new Headers(request.headers);
  requestHeaders.set(PICKER_HEADER, "1");
  return NextResponse.rewrite(url, { request: { headers: requestHeaders } });
}

export const config = {
  // Everything except Next's own assets, the image optimiser, and static files. The
  // picker needs those, and rewriting a request for one would serve it the page instead.
  matcher: ["/((?!_next/|api/|favicon.ico|.*\\.).*)"],
};
