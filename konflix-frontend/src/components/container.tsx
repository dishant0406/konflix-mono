import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

export default function Container({ children, className }: { children?: ReactNode; className?: string }) {
  return (
    <div className={cn("mx-auto w-full px-4 md:px-6 lg:px-8 max-w-[1400px] 2xl:max-w-[1600px]", className)}>
      {children}
    </div>
  );
}
