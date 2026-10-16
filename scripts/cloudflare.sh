#!/usr/bin/env bash
#
# Cloudflare DNS for Akidev, using the `cf` CLI.
#
# Points every hostname at the server. The site and the API are proxied (Cloudflare's cache and
# attack protection, and the cache rules in scripts/cloudflare-cache.sh); stream. is DNS only, so
# video goes to the viewer without passing through Cloudflare. Safe to re-run: an existing record
# for a hostname, including an old tunnel CNAME, is updated, not duplicated.
#
# Requires: `cf auth login` already done, and the akidev.live zone on this account.
#
# Usage:
#   SERVER_IP=203.0.113.7 ./scripts/cloudflare.sh              # every hostname
#   SERVER_IP=203.0.113.7 ONLY=anime.akidev.live ./scripts/cloudflare.sh   # one hostname
#   DRY_RUN=1 SERVER_IP=203.0.113.7 ./scripts/cloudflare.sh     # show the cf commands only
#
# Move a hostname only once Caddy has a certificate for it, or visitors get an error until it does.
# For a proxied hostname that means: update it, wait for https://<host> to answer, and only then
# do the next. If a certificate does not arrive, re-run with PROXIED=0 for that hostname.

set -euo pipefail

ZONE="${ZONE:-akidev.live}"
SERVER_IP="${SERVER_IP:?Set SERVER_IP to the IPv4 address of the server}"
case "$SERVER_IP" in
  *[!0-9.]*|*.*.*.*.*.*|'')
    echo "SERVER_IP must be an IPv4 address, got '$SERVER_IP'" >&2
    exit 1
    ;;
esac

# Hostname -> whether Cloudflare proxies it. The bare zone is the site; anime. and tv. are retired
# and the app redirects them to it, so they still need to reach it.
PROXIED_HOSTS=("anime.$ZONE" "tv.$ZONE" "api.$ZONE" "$ZONE")
DIRECT_HOSTS=("stream.$ZONE")

run() {
  if [ -n "${DRY_RUN:-}" ]; then
    printf '  [dry-run] %s\n' "$*"
  else
    "$@"
  fi
}

echo "Zone:   $ZONE"
echo "Server: $SERVER_IP"
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

point() {
  local host="$1" proxied="$2" ttl="$3" id body
  # Only an address or alias record is replaced, so a TXT or MX record on the same name is left.
  id="$(
    cf dns records list -q -z "$ZONE" | node -e "
      let d = ''
      process.stdin.on('data', (c) => (d += c)).on('end', () => {
        const list = JSON.parse(d)
        const match = list.find((r) => r.name === process.argv[1] && ['A', 'AAAA', 'CNAME'].includes(r.type))
        process.stdout.write(match ? match.id : '')
      })
    " "$host"
  )"
  body="{ \"type\": \"A\", \"name\": \"$host\", \"content\": \"$SERVER_IP\", \"proxied\": $proxied, \"ttl\": $ttl }"
  if [ -z "$id" ]; then
    echo "    $host: creating (proxied: $proxied)"
    run cf dns records create -z "$ZONE" --body "$body"
  else
    echo "    $host: updating (proxied: $proxied)"
    run cf dns records update "$id" -z "$ZONE" --body "$body"
  fi
}

echo "==> Pointing DNS at the server"
for host in "${PROXIED_HOSTS[@]}"; do
  if [ -z "${ONLY:-}" ] || [ "$ONLY" = "$host" ]; then
    point "$host" "$([ "${PROXIED:-1}" = 0 ] && echo false || echo true)" 1
  fi
done
for host in "${DIRECT_HOSTS[@]}"; do
  if [ -z "${ONLY:-}" ] || [ "$ONLY" = "$host" ]; then
    point "$host" false 300
  fi
done

echo "==> Ensuring TLS mode is full"
# Cloudflare connects to the server over https. 'flexible' would use plain http.
run cf zones settings edit ssl -z "$ZONE" --body '{"value":"full"}'
