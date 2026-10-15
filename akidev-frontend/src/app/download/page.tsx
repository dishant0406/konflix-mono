import type { Metadata } from "next";
import Link from "next/link";

import { Button } from "@/components/button";
import { Page, Section } from "@/components/prose";
import { REPO_URL } from "@/constants/site";
import { latestDownloads } from "@/lib/desktop-release";

// The newest releases are read from GitHub, at most once an hour.
export const revalidate = 3600;

const RELEASES = `${REPO_URL}/releases`;

export const metadata: Metadata = {
  title: "Download Akidev",
  description: "The Akidev desktop app for Windows and macOS.",
};

const link = "text-ink underline underline-offset-2 hover:text-accent";

export default async function DownloadPage() {
  const downloads = await latestDownloads();
  return (
    <Page
      title="Download Akidev"
      lead="The desktop app: the same catalogue as this site, with a native player. Free, open source, and it stores nothing but your settings and where you stopped watching."
    >
      <section className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-3 rounded-2xl border border-line bg-surface p-6">
          <h2 className="text-lg font-bold text-ink">Windows</h2>
          <p className="text-sm leading-6 text-dim">
            Windows 10 or later, 64-bit. Download the installer (<code>Akidev-…-setup.exe</code>); it installs for your
            account only and needs no administrator rights.
          </p>
          <Button href={downloads.windows?.url ?? RELEASES} className="mt-auto self-start">
            {downloads.windows ? `Download the installer (${downloads.windows.version})` : "Get the installer"}
          </Button>
        </div>
        <div className="flex flex-col gap-3 rounded-2xl border border-line bg-surface p-6">
          <h2 className="text-lg font-bold text-ink">macOS</h2>
          <p className="text-sm leading-6 text-dim">
            macOS 12 or later on Apple silicon. Open the disk image (<code>Akidev-…-arm64.dmg</code>) and drag Akidev
            onto Applications.
          </p>
          <Button href={downloads.mac?.url ?? RELEASES} look="secondary" className="mt-auto self-start">
            {downloads.mac ? `Download the disk image (${downloads.mac.version})` : "Get the disk image"}
          </Button>
        </div>
      </section>

      <Section title="Code signing policy">
        <p>
          Free code signing provided by{" "}
          <a href="https://signpath.io" className={link} rel="noopener">
            SignPath.io
          </a>
          , certificate by{" "}
          <a href="https://signpath.org" className={link} rel="noopener">
            SignPath Foundation
          </a>
          . The Windows installer is signed this way. Until a release carries that signature, Windows SmartScreen may
          warn that it is unrecognised: choose &ldquo;More info&rdquo;, then &ldquo;Run anyway&rdquo;.
        </p>
        <p>
          Only builds made from the public{" "}
          <a href={REPO_URL} className={link} rel="noopener">
            source repository
          </a>{" "}
          by its GitHub Actions workflow are submitted for signing. The project&apos;s maintainer is its only
          committer, reviewer and approver of signing requests.
        </p>
        <p>
          The macOS app is not signed with an Apple Developer ID yet. The first time you open it, macOS asks: open System
          Settings &rarr; Privacy &amp; Security and press &ldquo;Open Anyway&rdquo;.
        </p>
      </Section>

      <Section title="Privacy">
        <p>
          Akidev collects no personal data and has no accounts or analytics. See the{" "}
          <Link href="/privacy" className={link}>
            privacy policy
          </Link>{" "}
          for what the app does send over the network.
        </p>
      </Section>
    </Page>
  );
}
