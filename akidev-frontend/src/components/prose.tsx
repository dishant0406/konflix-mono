import type { ReactNode } from "react";

/** A plain reading page: a heading, then sections of text, in the site's quiet type. */
export function Page({ title, lead, children }: { title: string; lead?: string; children: ReactNode }) {
  return (
    <article className="gutter mx-auto max-w-3xl pb-10 pt-28">
      <h1 className="text-4xl font-black text-ink">{title}</h1>
      {lead && <p className="mt-3 text-[15px] leading-7 text-dim">{lead}</p>}
      <div className="mt-10 flex flex-col gap-10">{children}</div>
    </article>
  );
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3 text-[15px] leading-7 text-dim">
      <h2 className="text-lg font-bold text-ink">{title}</h2>
      {children}
    </section>
  );
}
