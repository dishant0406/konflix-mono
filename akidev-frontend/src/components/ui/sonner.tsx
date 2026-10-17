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
            "flex max-w-[560px] items-center gap-2 material-panel rounded-xl px-4 py-2.5 text-sm text-ink shadow-[0_0_0_1px_rgb(var(--ink)/0.08),0_24px_60px_rgb(0_0_0/0.55)]",
        },
      }}
    />
  );
}
