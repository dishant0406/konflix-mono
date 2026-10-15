import { notFound } from "next/navigation";

import { CatalogueView } from "@/components/browse";
import { CATALOGUES, catalogueForSlug } from "@/constants/site";

export function generateStaticParams() {
  return CATALOGUES.map((page) => ({ kind: page.slug }));
}

export const dynamicParams = false;

export default async function CataloguePage({ params }: { params: Promise<{ kind: string }> }) {
  const page = catalogueForSlug((await params).kind);
  if (!page) notFound();
  return <CatalogueView kind={page.kind} />;
}
