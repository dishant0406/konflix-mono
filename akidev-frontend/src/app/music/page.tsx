"use client";

import { Suspense } from "react";

import MusicPage from "@/components/music/music-page";

export default function Page() {
  // The page reads the search from the address, which needs a boundary while it is built.
  return (
    <Suspense>
      <MusicPage />
    </Suspense>
  );
}
