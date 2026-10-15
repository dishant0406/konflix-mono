#!/usr/bin/env bash
#
# Sets what Cloudflare may keep at the edge, so most requests are answered from near the visitor
# and never make the trip to the server. Safe to run again: it replaces the zone's cache rules.
#
# Requires: `cf auth login` already done, and the akidev.live zone on this account.
#
#   DRY_RUN=1 ./scripts/cloudflare-cache.sh    # validate the rules without applying them
#
# What is kept, and why each is safe to keep:
#
#   pages           /, /anime, /series, /films and /title/*. The same for everyone (a viewer's
#                   history is read in the browser), and the site says how long: half a minute,
#                   then refreshed in the background. Short, because a deploy changes the scripts a
#                   page names. Not the router's own requests (`_rsc`): those are a different answer
#                   at the same address, depending on the page the browser is on.
#   pictures        /v1/img. Each is a new file per address and never changes, so a month.
#   catalogue       the rest of /v1 except /v1/play. For as long as the API says (a minute to
#                   fifteen), so most never reach the server.
#
# The API sends no `Vary: Origin` (it allows one origin and says so on every answer), which is what
# lets these be kept: the edge ignores Vary, so a copy made for a request without an Origin header
# would otherwise reach the page with no CORS headers at all.

set -euo pipefail

ZONE="${ZONE:-akidev.live}"
SITE="${SITE_HOST:-$ZONE}"
API="${API_HOST:-api.$ZONE}"

RULES="$(cat <<JSON
[
  {
    "action": "set_cache_settings",
    "description": "Pages: kept for as long as the site says. Not the router's own requests, which are a different answer at the same address",
    "expression": "(http.host eq \"$SITE\" and (http.request.uri.path eq \"/\" or http.request.uri.path in {\"/anime\" \"/series\" \"/films\"} or starts_with(http.request.uri.path, \"/title/\")) and not http.request.uri.query contains \"_rsc\" and not any(http.request.headers.names[*] eq \"rsc\"))",
    "enabled": true,
    "action_parameters": { "cache": true, "edge_ttl": { "mode": "respect_origin" }, "browser_ttl": { "mode": "respect_origin" } }
  },
  {
    "action": "set_cache_settings",
    "description": "Resized posters and banners: immutable files, kept at the edge for a month",
    "expression": "(http.host eq \"$API\" and starts_with(http.request.uri.path, \"/v1/img\"))",
    "enabled": true,
    "action_parameters": { "cache": true, "edge_ttl": { "mode": "override_origin", "default": 2592000 }, "browser_ttl": { "mode": "respect_origin" } }
  },
  {
    "action": "set_cache_settings",
    "description": "Catalogue, search and title answers: kept for as long as the API says",
    "expression": "(http.host eq \"$API\" and starts_with(http.request.uri.path, \"/v1/\") and not starts_with(http.request.uri.path, \"/v1/play\") and not starts_with(http.request.uri.path, \"/v1/img\"))",
    "enabled": true,
    "action_parameters": { "cache": true, "edge_ttl": { "mode": "respect_origin" }, "browser_ttl": { "mode": "respect_origin" } }
  }
]
JSON
)"

echo "Zone: $ZONE ($SITE, $API)"

if [ -n "${DRY_RUN:-}" ]; then
  cf rulesets entrypoint update http_request_cache_settings -z "$ZONE" --name "Akidev cache rules" --rules "$RULES" --validate-only true -q > /dev/null
  echo "The rules are valid. Nothing was changed."
  exit 0
fi

cf rulesets entrypoint update http_request_cache_settings -z "$ZONE" --name "Akidev cache rules" --rules "$RULES" -q > /dev/null
echo "==> Cache rules set"

# Lets a data centre that has not got a copy ask a nearby one before it asks the server, which is
# what raises the share of requests answered at the edge.
cf cache settings smart-tiered-cache edit -z "$ZONE" --value on -q > /dev/null
echo "==> Smart Tiered Cache on"
