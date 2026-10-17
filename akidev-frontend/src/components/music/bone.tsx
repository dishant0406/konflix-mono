import { cn } from "@/lib/utils";

/** A grey block that pulses. The music skeletons are built from these on the same markup as the real pages. */
export default function Bone({ className, style }: { className?: string; style?: React.CSSProperties }) {
  return <div aria-hidden className={cn("animate-pulse rounded-md bg-white/10", className)} style={style} />;
}
