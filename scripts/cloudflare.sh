#!/usr/bin/env bash
#
# One-time Cloudflare setup for Akidev, using the `cf` CLI.
#
# Creates a named tunnel, points both hostnames at it, and prints the tunnel token
# that the cloudflared container needs. Safe to re-run: an existing tunnel and
# existing DNS records are updated rather than duplicated.
#
# Requires: `cf auth login` already done, and the akidev.live zone on this account.
#
# Usage:
#   ./scripts/cloudflare.sh              # create/update everything, print the token
#   ZONE=example.com ./scripts/cloudflare.sh
#   DRY_RUN=1 ./scripts/cloudflare.sh    # show the cf commands without running them

set -euo pipefail

ZONE="${ZONE:-akidev.live}"
TUNNEL_NAME="${TUNNEL_NAME:-konflix}"
# Hostname -> container service, routed by hostname in the tunnel ingress config.
#
# All three frontend hostnames point at the same container. The bare zone is the site; anime.
# and tv. are retired and the app redirects them to it, so they still need to reach it.
ANIME_HOST="${ANIME_HOST:-anime.$ZONE}"
TV_HOST="${TV_HOST:-tv.$ZONE}"
FRONTEND_HOST="${FRONTEND_HOST:-$ZONE}"
API_HOST="${API_HOST:-api.$ZONE}"

run() {
  if [ -n "${DRY_RUN:-}" ]; then
    printf '  [dry-run] %s\n' "$*"
  else
    "$@"
  fi
}

echo "Zone:     $ZONE"
echo "Tunnel:   $TUNNEL_NAME"
echo "Hostnames:"
echo "  $ANIME_HOST -> frontend:3000 (redirects to the site)"
echo "  $TV_HOST    -> frontend:3000 (redirects to the site)"
echo "  $FRONTEND_HOST -> frontend:3000 (the site)"
echo "  $API_HOST   -> api:9000"
echo

echo "==> Checking the zone is on this account"
if ! cf zones list -q | node -e "
  let d = ''
  process.stdin.on('data', (c) => (d += c)).on('end', () => {
    const zones = JSON.parse(d)
    process.exit(zones.some((z) => z.name === process.argv[1]) ? 0 : 1)
  })
" "$ZONE"; then
  echo "Error: $ZONE is not on this Cloudflare account. Check the zone, or add it first." >&2
  exit 1
fi

echo "==> Finding or creating the tunnel"
TUNNEL_ID="$(
  cf tunnels list -q --name "$TUNNEL_NAME" | node -e "
    let d = ''
    process.stdin.on('data', (c) => (d += c)).on('end', () => {
      const list = JSON.parse(d)
      const match = list.find((t) => t.name === process.argv[1] && !t.deleted_at)
      process.stdout.write(match ? match.id : '')
    })
  " "$TUNNEL_NAME"
)"

if [ -z "$TUNNEL_ID" ]; then
  echo "    none found, creating one"
  run cf tunnels create --name "$TUNNEL_NAME" --config-source cloudflare
  if [ -z "${DRY_RUN:-}" ]; then
    TUNNEL_ID="$(
      cf tunnels list -q --name "$TUNNEL_NAME" | node -e "
        let d = ''
        process.stdin.on('data', (c) => (d += c)).on('end', () => {
          const list = JSON.parse(d)
          const match = list.find((t) => t.name === process.argv[1] && !t.deleted_at)
          process.stdout.write(match ? match.id : '')
        })
      " "$TUNNEL_NAME"
    )"
  else
    TUNNEL_ID="<created>"
  fi
fi

echo "    tunnel id: $TUNNEL_ID"

echo "==> Setting the ingress rules"
# Ordered: the two hostnames, then a catch-all. Cloudflare requires the catch-all to
# be last, and it must exist or unmatched requests are refused with no explanation.
run cf tunnels config update "$TUNNEL_ID" --body "$(cat <<JSON
{
  "config": {
    "ingress": [
      { "hostname": "$ANIME_HOST", "service": "http://frontend:3000" },
      { "hostname": "$TV_HOST", "service": "http://frontend:3000" },
      { "hostname": "$FRONTEND_HOST", "service": "http://frontend:3000" },
      { "hostname": "$API_HOST", "service": "http://api:9000" },
      { "service": "http_status:404" }
    ]
  }
}
JSON
)"

echo "==> Pointing DNS at the tunnel"
# A proxied CNAME to the tunnel. No origin IP is published and no certificate has to
# be issued on the server: the edge terminates TLS.
for host in "$ANIME_HOST" "$TV_HOST" "$FRONTEND_HOST" "$API_HOST"; do
  RECORD_ID="$(
    cf dns records list -q -z "$ZONE" | node -e "
      let d = ''
      process.stdin.on('data', (c) => (d += c)).on('end', () => {
        const list = JSON.parse(d)
        const match = list.find((r) => r.name === process.argv[1])
        process.stdout.write(match ? match.id : '')
      })
    " "$host"
  )"

  if [ -z "$RECORD_ID" ]; then
    echo "    $host: creating"
    run cf dns records create -z "$ZONE" --body "$(cat <<JSON
{ "type": "CNAME", "name": "$host", "content": "$TUNNEL_ID.cfargotunnel.com", "proxied": true, "ttl": 1 }
JSON
)"
  else
    echo "    $host: updating"
    run cf dns records update "$RECORD_ID" -z "$ZONE" --body "$(cat <<JSON
{ "type": "CNAME", "name": "$host", "content": "$TUNNEL_ID.cfargotunnel.com", "proxied": true, "ttl": 1 }
JSON
)"
  fi
done

echo "==> Ensuring TLS mode is full"
# The tunnel terminates TLS at the edge and speaks http to the container, so the
# origin connection is already encrypted. 'flexible' would downgrade it.
run cf zones settings edit ssl -z "$ZONE" --body '{"value":"full"}'

echo
echo "==> Tunnel token"
if [ -n "${DRY_RUN:-}" ]; then
  echo "    [dry-run] cf tunnels token get $TUNNEL_ID"
else
  TOKEN="$(cf tunnels token get "$TUNNEL_ID" -q | node -e "
    let d = ''
    process.stdin.on('data', (c) => (d += c)).on('end', () => {
      const body = JSON.parse(d)
      process.stdout.write(typeof body === 'string' ? body : (body.token || ''))
    })
  ")"
  if [ -z "$TOKEN" ]; then
    echo "    Could not read the token automatically. Run: cf tunnels token get $TUNNEL_ID" >&2
    exit 1
  fi
  echo "    Add this as the CLOUDFLARE_TUNNEL_TOKEN repository secret:"
  echo
  echo "    $TOKEN"
  echo
  echo "    This is a secret. It is not stored in the repository."
fi
