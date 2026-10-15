import type { Metadata } from "next";
import Link from "next/link";

import { Button } from "@/components/button";
import { Page, Section } from "@/components/prose";
import { REPO_URL } from "@/constants/site";
import { latestDownloads, type Download } from "@/lib/desktop-release";

// The newest releases are read from GitHub, at most once an hour.
export const revalidate = 3600;

const RELEASES = `${REPO_URL}/releases`;

export const metadata: Metadata = {
  title: "Download Akidev",
  description: "The Akidev desktop app for Windows and macOS.",
};

const link = "text-ink underline decoration-ink/30 underline-offset-4 transition-colors hover:decoration-ink";

export default async function DownloadPage() {
  const downloads = await latestDownloads();
  return (
    <Page
      title="Akidev for desktop."
      lead="The same catalogue as the site, with a native player. Free and open source, and it keeps nothing but your settings and where you stopped."
    >
      <Platform
        name="Windows"
        detail="Windows 10 or later, 64-bit. An installer for your account; no administrator rights."
        download={downloads.windows}
        label="Download installer"
        primary
      />
      <Platform
        name="macOS"
        detail="macOS 12 or later, Apple silicon. Open the disk image and drag Akidev to Applications."
        download={downloads.mac}
        label="Download disk image"
      />

      <Section title="Code signing">
        <p>
          Free code signing provided by{" "}
          <a href="https://signpath.io" className={link} rel="noopener">
            SignPath.io
          </a>
          , certificate by{" "}
          <a href="https://signpath.org" className={link} rel="noopener">
            SignPath Foundation
          </a>
          . Windows may still warn about a release that does not carry the signature yet: choose More info, then Run
          anyway.
        </p>
        <p>
          Only builds of the public{" "}
          <a href={REPO_URL} className={link} rel="noopener">
            source
          </a>{" "}
          made by its GitHub Actions workflow are submitted for signing. The maintainer is the only committer, reviewer
          and approver.
        </p>
        <p>
          The macOS app is not signed with an Apple Developer ID yet. The first time, open System Settings, Privacy and
          Security, and press Open Anyway.
        </p>
      </Section>

      <Section title="Privacy">
        <p>
          No accounts, no analytics. What the app does send over the network is in the{" "}
          <Link href="/privacy" className={link}>
            privacy policy
          </Link>
          .
        </p>
      </Section>
    </Page>
  );
}

function Platform({
  name,
  detail,
  download,
  label,
  primary,
}: {
  name: string;
  detail: string;
  download: Download | null;
  label: string;
  primary?: boolean;
}) {
  return (
    <section className="flex flex-col gap-5 border-b border-line py-8 sm:flex-row sm:items-center sm:justify-between sm:gap-10">
      <div className="max-w-md">
        <h2 className="text-xl font-bold text-ink">{name}</h2>
        <p className="mt-1.5 text-sm leading-6 text-dim">{detail}</p>
      </div>
      <div className="flex shrink-0 flex-col gap-1.5 sm:items-end">
        <Button href={download?.url ?? RELEASES} look={primary ? "primary" : "secondary"}>
          {label}
        </Button>
        {download && <span className="text-xs text-faint">Version {download.version}</span>}
      </div>
    </section>
  );
}
