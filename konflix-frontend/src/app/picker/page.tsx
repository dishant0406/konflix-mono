import { ArrowRight } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import { Suspense } from "react";

import Container from "@/components/container";
import { PICKER_SITES, siteUrl, type SiteConfig } from "@/constants/site";
import { request } from "@/lib/api";
import { GET_MEDIA_LIST } from "@/lib/queries";
import { mediaPoster } from "@/lib/utils";
import type { MediaList, MediaPage } from "@/types/media";

// The picker belongs to no site, so it cannot use the site context. It is a server
// component, and the artwork is streamed rather than awaited: the shell renders
// immediately and the previews follow, so a slow or unreachable API cannot hold up the
// page. That also keeps the frontend's health check independent of the API.

export const dynamic = "force-dynamic";

const PREVIEW_SIZE = 5;

/** A few popular titles for a card's artwork, or none if the API did not answer. */
async function preview(contentType: SiteConfig["contentType"]): Promise<MediaList[]> {
  try {
    const data = await request<{ mediaList: MediaPage }>(GET_MEDIA_LIST, {
      input: { contentType, sort: "popularity", perPage: PREVIEW_SIZE },
    });
    return (data.mediaList.results ?? []).filter((item) => mediaPoster(item.coverImage));
  } catch {
    return [];
  }
}

function ArtworkRow({ media }: { media: MediaList[] }) {
  return (
    <div className="flex gap-2 overflow-hidden p-3">
      {media.length > 0 ? (
        media.map((item) => (
          <div
            key={item.id}
            className="relative aspect-[2/3] w-1/5 shrink-0 overflow-hidden rounded-md bg-placeholder"
          >
            <Image
              src={mediaPoster(item.coverImage)!}
              alt=""
              fill
              sizes="120px"
              className="object-cover transition-transform duration-300 group-hover:scale-105"
            />
          </div>
        ))
      ) : (
        // No artwork: the card still has to look deliberate rather than broken.
        <div className="aspect-[2/3] w-full rounded-md border border-dashed border-border bg-secondary/40" />
      )}
    </div>
  );
}

async function StreamedArtwork({ contentType }: { contentType: SiteConfig["contentType"] }) {
  return <ArtworkRow media={await preview(contentType)} />;
}

function ServiceCard({ site, blurb }: { site: SiteConfig; blurb: string }) {
  return (
    <Link
      href={siteUrl(site)}
      className="group relative flex flex-col overflow-hidden rounded-xl border border-border bg-card transition-colors hover:border-primary/60"
    >
      <Suspense fallback={<ArtworkRow media={[]} />}>
        <StreamedArtwork contentType={site.contentType} />
      </Suspense>

      <div className="flex flex-1 flex-col gap-2 px-5 pb-5">
        <h2 className="text-xl font-bold text-foreground">{site.appName}</h2>
        <p className="text-sm text-muted-foreground">{blurb}</p>
        <span className="mt-auto inline-flex items-center gap-2 pt-3 text-sm font-semibold text-primary">
          Open
          <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-1" />
        </span>
      </div>
    </Link>
  );
}

export default function PickerPage() {
  return (
    <div className="flex min-h-screen flex-col">
      <header className="flex items-center justify-center py-14">
        <span className="bg-gradient-to-r from-foreground to-primary bg-clip-text text-4xl font-black tracking-widest text-transparent">
          KONFLIX
        </span>
      </header>

      <Container className="flex flex-1 flex-col items-center gap-8 pb-20">
        <p className="text-center text-sm text-muted-foreground">Choose a service.</p>

        <div className="grid w-full max-w-4xl gap-6 md:grid-cols-2">
          {PICKER_SITES.map((entry) => (
            <ServiceCard key={entry.site.subdomain} site={entry.site} blurb={entry.blurb} />
          ))}
        </div>
      </Container>

      <footer className="border-t border-border py-6 text-center text-xs text-muted-foreground">
        Konflix stores no media on its own servers. Playback links are resolved from
        public sources at the moment you press play.
      </footer>
    </div>
  );
}
