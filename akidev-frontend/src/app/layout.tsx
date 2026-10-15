import type { Metadata } from "next";
import "./globals.css";

import Footer from "@/components/footer";
import Navbar from "@/components/navbar";
import { Toaster } from "@/components/ui/sonner";
import { SITE_DOMAIN } from "@/constants/site";
import { appFont } from "@/lib/fonts";
import { siteFromRequest } from "@/lib/site-server";
import QueryProvider from "@/providers/query-provider";
import SiteProvider from "@/providers/site-provider";

// The host decides which site this request is for, so the chrome and the copy are
// resolved per request rather than baked into one build. The picker host belongs to no
// site and renders bare - it has its own header and footer.

export async function generateMetadata(): Promise<Metadata> {
  const site = await siteFromRequest();

  // The picker is the zone's front door, so it carries the zone's own name.
  if (!site) {
    const title = "Akidev | Choose a service";
    const description = "Choose between Akidev Anime and Akidev TV.";
    return {
      applicationName: "Akidev",
      title,
      description,
      icons: { icon: "/favicon.ico" },
      openGraph: { type: "website", siteName: "Akidev", title, description },
      twitter: { card: "summary", title, description },
    };
  }

  return {
    applicationName: site.appName,
    title: site.title,
    description: site.description,
    icons: { icon: "/favicon.ico" },
    // Absolute, so Open Graph URLs resolve. Derived from the site's own subdomain
    // rather than from the request, so a preview fetched from anywhere still points at
    // the canonical origin.
    metadataBase: new URL(`https://${site.subdomain}.${SITE_DOMAIN}`),
    alternates: { canonical: "/" },
    openGraph: {
      type: "website",
      siteName: site.appName,
      title: site.title,
      description: site.description,
    },
    twitter: { card: "summary", title: site.title, description: site.description },
  };
}

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const site = await siteFromRequest();

  return (
    <html lang="en">
      <body className={`${appFont.className} min-h-screen max-w-[100vw] overflow-x-hidden bg-background antialiased`}>
        <QueryProvider>
          {site ? (
            <SiteProvider site={site}>
              <Navbar />
              <main className="min-h-[70vh] pt-[4.25rem]">{children}</main>
              <Footer site={site} />
            </SiteProvider>
          ) : (
            children
          )}
          <Toaster />
        </QueryProvider>
      </body>
    </html>
  );
}
