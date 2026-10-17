"use client";

import { cn } from "@/lib/utils";

/** A thin range: the track fills as far as the value, and the thumb shows on hover (see `.slider`). */
export default function Slider({
  label,
  value,
  max,
  step = 1,
  onChange,
  className,
}: {
  label: string;
  value: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  className?: string;
}) {
  const fill = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return (
    <input
      type="range"
      aria-label={label}
      min={0}
      max={max || 1}
      step={step}
      value={Math.min(value, max || 1)}
      onChange={(event) => onChange(Number(event.target.value))}
      style={{ "--fill": `${fill}%` } as React.CSSProperties}
      className={cn("slider w-full", className)}
    />
  );
}
