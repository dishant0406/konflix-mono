import posthog from "posthog-js";

/**
 * Product analytics, through PostHog. Nothing here may break the page, and with no project token
 * (local development, a build without one) nothing is sent: `instrumentation-client.ts` then never
 * starts PostHog, and these calls do nothing.
 */
export function track(event: string, properties?: Record<string, string | number | boolean | null>) {
  try {
    if (posthog.__loaded) posthog.capture(event, properties);
  } catch {
    // Analytics never gets in the way of watching something.
  }
}

/** Reports an error the page caught itself, so it appears in PostHog's error tracking. */
export function reportError(error: unknown, properties?: Record<string, string | number | boolean | null>) {
  try {
    if (posthog.__loaded) posthog.captureException(error, properties);
  } catch {
    // As above.
  }
}
