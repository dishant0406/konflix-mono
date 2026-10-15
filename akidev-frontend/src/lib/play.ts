// What the watch page needs to know about playing, kept free of React so it can be tested.

import { ApiError } from "./api.ts";

const HEVC = 'video/mp4; codecs="hvc1.1.6.L120.90"';

let hevc: boolean | undefined;

/** Whether this browser plays HEVC, so the server can keep it as it is. Asked once; false on the server. */
export function canPlayHevc(): boolean {
  if (hevc === undefined) {
    if (typeof MediaSource === "undefined") return false;
    hevc = MediaSource.isTypeSupported(HEVC);
  }
  return hevc;
}

/** What to tell the viewer when the play request itself failed. */
export function playErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.status === 503) return "The server is busy, try again in a moment.";
  return error instanceof Error ? error.message : "The API did not respond.";
}
