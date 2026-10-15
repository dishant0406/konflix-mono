import Link from "next/link";

import { Button } from "@/components/ui/button";
import { siteFromRequest } from "@/lib/site-server";

export default async function NotFound() {
  // The picker host belongs to no site, so there is no item label to name. It has one
  // route and does not link anywhere that can 404.
  const site = await siteFromRequest();
  return (
    <div className="flex h-[70vh] flex-col items-center justify-center gap-4 px-6 text-center">
      <h1 className="text-3xl font-black text-foreground">Not found</h1>
      <p className="max-w-md text-sm text-muted-foreground">
        {site ? `That page does not exist. The ${site.itemLabel} may have been removed, or the link may be wrong.` : "That page does not exist."}
      </p>
      <Button asChild>
        <Link href="/">Back to home</Link>
      </Button>
    </div>
  );
}
