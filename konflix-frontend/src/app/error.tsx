"use client";

import Link from "next/link";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";

export default function Error({ error, reset }: { error: Error; reset: () => void }) {
  useEffect(() => {
    // The detail stays in the console for debugging; the page shows something
    // actionable rather than the raw error.
    console.error(error);
  }, [error]);

  return (
    <div className="flex h-[70vh] flex-col items-center justify-center gap-4 px-6 text-center">
      <h1 className="text-2xl font-bold text-white">Something went wrong</h1>
      <p className="max-w-md text-sm text-gray-400">
        The page could not be loaded. This is usually temporary.
      </p>
      <div className="flex gap-3">
        <Button onClick={reset}>Try again</Button>
        <Button variant="secondary" asChild>
          <Link href="/">Back to home</Link>
        </Button>
      </div>
    </div>
  );
}
