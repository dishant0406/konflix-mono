"use client";

import { useEffect } from "react";

import { ErrorState } from "@/components/states";
import { reportError } from "@/lib/analytics";

export default function Error({ error, reset }: { error: Error; reset: () => void }) {
  useEffect(() => {
    // The detail stays in the console; the page shows something actionable.
    console.error(error);
    reportError(error, { source: "error-boundary" });
  }, [error]);

  return (
    <div className="pt-32">
      <ErrorState title="Something went wrong" detail="The page could not be loaded. This is usually temporary." onRetry={reset} />
    </div>
  );
}
