import posthog from "posthog-js";

// The project token is public by design (it only lets events in), and Next inlines it at build
// time. Without one, PostHog never starts.
const token = process.env.NEXT_PUBLIC_POSTHOG_TOKEN;

if (token) {
  posthog.init(token, {
    // Events go to our own domain and are forwarded by the rewrites in next.config.mjs, so a
    // blocker list that knows posthog.com does not hide them, and the visitor's browser talks
    // to one host only.
    api_host: "/ingest",
    ui_host: "https://us.posthog.com",
    defaults: "2026-05-30",
    // Visitors are anonymous and stay that way: there is nothing to sign in to.
    person_profiles: "identified_only",
    // localStorage, not a cookie: the privacy policy promises Akidev sets none.
    persistence: "localStorage",
    // Pages are counted as the router moves between them.
    capture_pageview: "history_change",
    // Uncaught errors and unhandled rejections go to error tracking.
    capture_exceptions: true,
    // Only the events named in src/lib/analytics.ts callers; no click scraping, no recordings.
    autocapture: false,
    disable_session_recording: true,
    // The privacy policy says Do Not Track is honoured.
    respect_dnt: true,
  });
}
