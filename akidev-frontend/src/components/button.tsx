import Link from "next/link";
import type { ComponentProps, ReactNode } from "react";

import { cn } from "@/lib/utils";

type Look = "primary" | "secondary";

const LOOKS: Record<Look, string> = {
  // The one action on a screen: white, like a cinema's marquee.
  primary: "bg-ink text-canvas font-bold hover:bg-ink/80 active:bg-ink/70",
  // Everything else: translucent, so it sits on artwork as well as on the canvas.
  secondary: "bg-ink/[0.12] text-ink font-semibold hover:bg-ink/20 active:bg-ink/[0.28]",
};

const BASE =
  "inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 rounded-md px-5 py-2.5 text-[15px] transition-colors disabled:pointer-events-none disabled:opacity-40";

type Props = {
  look?: Look;
  icon?: ReactNode;
  href?: string;
  className?: string;
  children?: ReactNode;
} & Omit<ComponentProps<"button">, "className" | "children">;

export function Button({ look = "primary", icon, href, className, children, ...rest }: Props) {
  const classes = cn(BASE, LOOKS[look], className);
  const content = (
    <>
      {icon}
      {children}
    </>
  );
  if (href) {
    return (
      <Link href={href} className={classes}>
        {content}
      </Link>
    );
  }
  return (
    <button type="button" className={classes} {...rest}>
      {content}
    </button>
  );
}

/** A round icon-only button, for the bars and the player. */
export function IconButton({
  label,
  className,
  children,
  ...rest
}: { label: string; className?: string; children: ReactNode } & Omit<ComponentProps<"button">, "className" | "children">) {
  return (
    <button
      type="button"
      aria-label={label}
      className={cn(
        "flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full text-ink transition-colors hover:bg-ink/[0.14] active:bg-ink/[0.22] disabled:opacity-35",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

/** A navigation tab: text only, the active one bright with an accent underline. */
export function Tab({
  active,
  className,
  children,
  ...rest
}: { active: boolean; className?: string; children: ReactNode } & Omit<ComponentProps<"button">, "className" | "children">) {
  return (
    <button
      type="button"
      className={cn(
        "flex cursor-pointer flex-col items-center gap-1 px-1 py-1 text-sm transition-colors",
        active ? "font-bold text-ink" : "text-dim hover:text-ink",
        className,
      )}
      {...rest}
    >
      {children}
      <span className={cn("h-0.5 w-full rounded-full", active && "bg-accent")} />
    </button>
  );
}
