"use client";

import { Toaster as Sonner } from "sonner";

// A toast is the app's own: a raised pill at the bottom, centred.
export function Toaster() {
  return (
    <Sonner
      theme="dark"
      position="bottom-center"
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            "flex max-w-[560px] items-center gap-2 rounded-2xl border border-line bg-raised px-4 py-2.5 text-sm text-ink shadow-lg",
        },
      }}
    />
  );
}
