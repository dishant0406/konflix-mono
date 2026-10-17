import { Music } from "lucide-react";
import Image from "next/image";

import { cn } from "@/lib/utils";

/** A square cover, or a note where there is none. `sizes` is what the browser should expect to draw. */
export default function Cover({
  src,
  sizes,
  round,
  priority,
  className,
}: {
  src: string | null;
  sizes: string;
  round?: boolean;
  priority?: boolean;
  className?: string;
}) {
  return (
    <div className={cn("relative aspect-square overflow-hidden bg-white/10", round ? "rounded-full" : "rounded-md", className)}>
      {src ? (
        <Image src={src} alt="" fill sizes={sizes} quality={70} priority={priority} className="object-cover" />
      ) : (
        <Music className="absolute left-1/2 top-1/2 size-1/3 -translate-x-1/2 -translate-y-1/2 text-white/40" />
      )}
    </div>
  );
}
