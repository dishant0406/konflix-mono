import Link from "next/link";
import type { ComponentProps, ReactNode } from "react";

import { cn } from "@/lib/utils";

type Look = "primary" | "secondary" | "ghost";

const LOOKS: Record<Look, string> = {
  // The one action on a screen: warm white, like a cinema's marquee.
  primary: "bg-ink text-canvas font-semibold hover:bg-ink/85 active:bg-ink/75",
  // Everything else: a hairline, no fill until it is reached, so it sits on artwork and on the canvas.
  secondary: "border border-ink/20 text-ink font-medium hover:bg-wash-2 hover:border-ink/30 active:bg-wash-3",
  // A quiet action in a line of text: underlined when reached.
  ghost: "px-1 text-dim font-medium underline-offset-4 hover:text-ink hover:underline",
};

const BASE =
  "inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 rounded-md px-5 py-2.5 text-[14px] transition-[colors,transform] duration-150 ease-spring active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40";

type Props = {
  look?: Look;
  icon?: ReactNode;
  href?: string;
  /** Fetch the page ahead, for the actions a screen exists to offer. */
  prefetch?: boolean;
  className?: string;
  children?: ReactNode;
} & Omit<ComponentProps<"button">, "className" | "children">;

export function Button({ look = "primary", icon, href, prefetch, className, children, ...rest }: Props) {
  const classes = cn(BASE, LOOKS[look], look === "ghost" && "py-1", className);
  const content = (
    <>
      {icon}
      {children}
    </>
  );
  if (href) {
    return (
      <Link href={href} prefetch={prefetch} className={classes}>
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
        "flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full text-ink transition-[colors,transform] duration-150 ease-spring active:scale-90 hover:bg-wash-2 active:bg-wash-3 disabled:opacity-35",
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
        active ? "font-semibold text-ink" : "text-dim hover:text-ink",
        className,
      )}
      {...rest}
    >
      {children}
      <span className={cn("h-0.5 w-full rounded-full", active && "bg-accent")} />
    </button>
  );
}

/** A keyboard shortcut drawn as the key it is: "Ctrl K". */
export function Keycap({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded-sm border border-ink/15 bg-wash px-1.5 font-sans text-[11px] font-medium leading-none text-dim shadow-[inset_0_-1px_0_rgb(var(--ink)/0.08)]",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
