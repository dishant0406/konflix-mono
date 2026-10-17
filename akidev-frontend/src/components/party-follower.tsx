"use client";

import { usePathname, useRouter } from "next/navigation";
import { useEffect } from "react";

import { watchPath } from "@/constants/routes";
import { parseMediaId } from "@/lib/media-id";
import { useParty } from "@/providers/party";

/**
 * Takes a guest to the page of what the host starts. It happens when the host changes what is
 * playing, once: a guest who then goes somewhere else is left there, and the player offers to
 * rejoin.
 */
export default function PartyFollower() {
  const party = useParty();
  const router = useRouter();
  const pathname = usePathname();
  const id = party.role === "guest" && party.item?.kind === "video" ? party.item.id : null;

  useEffect(() => {
    const parsed = parseMediaId(id ?? undefined);
    if (!id || !parsed) return;
    const page = watchPath(parsed.key, id);
    if (pathname !== page) router.push(page);
    // Only a change of what is playing moves the guest, not every page they visit.
  }, [id]); // eslint-disable-line react-hooks/exhaustive-deps

  return null;
}
