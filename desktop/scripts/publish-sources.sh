#!/usr/bin/env bash
#
# Publishes sources/sources.json to the Cloudflare R2 bucket the app reads it from.
#
# It checks the document, refuses one that is not newer than what is live, signs it with the
# private key and uploads the document and its signature. Apps pick it up within six hours.
#
# Requires: the cf CLI logged in to the Cloudflare account, and the signing key.
#
# Usage:
#   ./scripts/publish-sources.sh
#   BUCKET=my-bucket KEY=/path/to/sources.key ./scripts/publish-sources.sh

set -euo pipefail
cd "$(dirname "$0")/.."

BUCKET="${BUCKET:-akidev-sources}"
KEY="${KEY:-.secrets/sources.key}"
URL="${URL:-https://sources.akidev.live}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tool=(cargo run -q -p akidev-core --bin akidev-sources --)

version() { grep -o '"version": *[0-9]*' | head -1 | grep -o '[0-9]*$'; }
new="$(version < sources/sources.json)"
live="$(curl -fsS "$URL/sources.json" | version || echo 0)"
if [ "$new" -le "${live:-0}" ]; then
  echo "sources/sources.json is version $new, and version ${live:-0} is live. Raise \"version\" first." >&2
  exit 1
fi

"${tool[@]}" sign "$KEY" sources/sources.json "$work/sources.sig"
"${tool[@]}" check sources/sources.json "$work/sources.sig"

put() {
  cf r2 objects put "$1" --bucket-name "$BUCKET" --file "$2" --content-type "$3" > /dev/null
}
put sources.json sources/sources.json application/json
put sources.sig "$work/sources.sig" text/plain
echo "Published version $new to $URL"
