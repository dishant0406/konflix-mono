import { notFound } from "next/navigation";

import { CatalogueView } from "@/components/browse";
import { CATALOGUES, DEFAULT_SORT, catalogueForSlug } from "@/constants/site";
import { serverGet } from "@/lib/api-server";
import type { Page } from "@/types/media";

export function generateStaticParams() {
  return CATALOGUES.map((page) => ({ kind: page.slug }));
}

export const dynamicParams = false;

// Rendered for each request with the first page already in it; see app/page.tsx.
export const dynamic = "force-dynamic";

export default async function CataloguePage({ params }: { params: Promise<{ kind: string }> }) {
  const page = catalogueForSlug((await params).kind);
  if (!page) notFound();
  const first = await serverGet<Page>(`/v1/catalogue?kind=${page.kind}&sort=${DEFAULT_SORT}&page=1`, 60);
  return <CatalogueView kind={page.kind} initial={first ?? undefined} />;
}
