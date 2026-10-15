import Link from "next/link";

import { ATTRIBUTION } from "@/constants/site";

/** One quiet block at the foot of the page: the attribution the providers' licences ask for, and the pages about the app. */
export default function Footer() {
  return (
    <footer className="gutter flex flex-col gap-3 pb-10 pt-6">
      <p className="max-w-3xl text-xs leading-5 text-faint">{ATTRIBUTION}</p>
      <nav className="flex gap-5 text-xs text-faint">
        <Link href="/download" className="transition-colors hover:text-ink">
          Download the app
        </Link>
        <Link href="/privacy" className="transition-colors hover:text-ink">
          Privacy
        </Link>
      </nav>
    </footer>
  );
}
