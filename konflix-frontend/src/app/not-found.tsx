import Link from "next/link";

import { Button } from "@/components/ui/button";
import { SITE } from "@/constants/site";

export default function NotFound() {
  return (
    <div className="flex h-[70vh] flex-col items-center justify-center gap-4 px-6 text-center">
      <h1 className="text-3xl font-black text-foreground">Not found</h1>
      <p className="max-w-md text-sm text-muted-foreground">
        That page does not exist. The {SITE.itemLabel} may have been removed, or the link may be wrong.
      </p>
      <Button asChild>
        <Link href="/">Back to home</Link>
      </Button>
    </div>
  );
}
