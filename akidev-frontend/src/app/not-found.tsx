import { Button } from "@/components/button";

export default function NotFound() {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 pb-16 pt-40 text-center">
      <h1 className="text-4xl font-black text-ink">Not found</h1>
      <p className="max-w-md text-sm text-dim">That page does not exist. The title may have been removed, or the link may be wrong.</p>
      <Button href="/" className="mt-3">
        Back to home
      </Button>
    </div>
  );
}
