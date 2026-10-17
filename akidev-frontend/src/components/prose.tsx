import type { ReactNode } from "react";

/** A reading page: a large quiet heading, then rows of label and text divided by hairlines. */
export function Page({ title, lead, children }: { title: string; lead?: string; children: ReactNode }) {
  return (
    <article className="gutter mx-auto max-w-4xl pb-16 pt-32 md:pt-40">
      <h1 className="text-balance font-title text-4xl font-semibold leading-[1.12] tracking-[-0.02em] text-ink md:text-6xl">{title}</h1>
      {lead && <p className="mt-5 max-w-xl text-pretty text-base leading-7 text-dim">{lead}</p>}
      <div className="mt-16 border-t border-line">{children}</div>
    </article>
  );
}

/** The label sits in the margin on a wide screen and above the text on a narrow one. */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid gap-3 border-b border-line py-8 md:grid-cols-[11rem_1fr] md:gap-10">
      <h2 className="caps text-ink">{title}</h2>
      <div className="flex max-w-xl flex-col gap-3 text-[15px] leading-7 text-dim">{children}</div>
    </section>
  );
}
