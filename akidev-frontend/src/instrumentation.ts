import type { Instrumentation } from "next";

/**
 * Errors thrown while the server renders a page or answers a request, sent to PostHog's error
 * tracking. The browser's own errors are reported by `instrumentation-client.ts`.
 */
export const onRequestError: Instrumentation.onRequestError = async (error, request) => {
  const token = process.env.NEXT_PUBLIC_POSTHOG_TOKEN;
  // posthog-node needs Node; the edge runtime (the middleware) has no use for it.
  if (!token || process.env.NEXT_RUNTIME !== "nodejs") return;
  try {
    const { PostHog } = await import("posthog-node");
    const client = new PostHog(token, {
      host: "https://us.i.posthog.com",
      flushAt: 1,
      flushInterval: 0,
      // Server errors belong to no visitor, so they carry no address.
      disableGeoip: true,
    });
    client.captureException(error, undefined, { path: request.path, method: request.method, source: "server" });
    await client.shutdown();
  } catch {
    // Reporting must never turn one failure into two.
  }
};
