"use client";

import type { ComponentProps } from "react";

import { Button } from "@/components/button";
import { track } from "@/lib/analytics";

/** A download link that says which platform and version was asked for. */
export function DownloadButton({
  platform,
  version,
  ...rest
}: { platform: string; version: string | null } & ComponentProps<typeof Button>) {
  return <Button {...rest} onClick={() => track("download_clicked", { platform, version })} />;
}
