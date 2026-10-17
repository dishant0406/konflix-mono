"use client";

import { useEffect, useState } from "react";

import { dominantColor } from "@/lib/dominant-color";

/** The colour of a cover as "r g b", once it is known. Neutral grey until then, so the page does not flash. */
export function useTint(cover: string | null | undefined): string {
  const [tint, setTint] = useState("70 70 70");
  useEffect(() => {
    let current = true;
    if (cover) void dominantColor(cover).then((found) => current && found && setTint(found));
    return () => {
      current = false;
    };
  }, [cover]);
  return tint;
}
