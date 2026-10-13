import Link from "next/link";

import { Button } from "@/components/ui/button";

export default function NotFound() {
  return (
    <div className="flex h-[70vh] flex-col items-center justify-center gap-4 px-6 text-center">
      <h1 className="text-3xl font-black text-white">Not found</h1>
      <p className="max-w-md text-sm text-gray-400">
        That page does not exist. The anime may have been removed, or the link may be wrong.
      </p>
      <Button asChild>
        <Link href="/">Back to home</Link>
      </Button>
    </div>
  );
}
