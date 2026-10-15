"use client";

import { useSearchParams } from "next/navigation";
import { Suspense } from "react";

import { CatalogueView, SearchView } from "@/components/browse";

function Results() {
  const term = useSearchParams().get("q")?.trim() ?? "";
  // Nothing typed is a browse of everything, as in the app.
  return term ? <SearchView term={term} /> : <CatalogueView kind={null} />;
}

export default function SearchPage() {
  return (
    <Suspense>
      <Results />
    </Suspense>
  );
}
