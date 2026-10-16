#!/usr/bin/env bash
#
# Deploys the Akidev stack to the server. Runs ON the server, from the repository
# checkout, and is what .github/workflows/deploy.yml invokes over SSH.
#
# The images are built on the server rather than in CI and pushed: the host is the
# only place with the CPU architecture the images must run on, and it avoids both a
# registry and an image transfer for every deploy.
#
# The revision comes from the image tag, not from git, so this does not depend on git
# being installed or on the checkout being a repository.
#
# Only Docker and curl are required on the host; everything else runs in a container.
#
# Usage:
#   ./scripts/deploy.sh                # build and start
#   PULL=1 ./scripts/deploy.sh         # also pull the base images first
#   SKIP_BUILD=1 ./scripts/deploy.sh   # restart the current images without rebuilding

set -euo pipefail

cd "$(dirname "$0")/.."

ENV_FILE="${ENV_FILE:-.env.deploy}"
COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-180}"
IMAGE_TAG="${IMAGE_TAG:-$(date -u +%Y%m%d%H%M%S)}"

if [ ! -f "$ENV_FILE" ]; then
  echo "Error: $ENV_FILE not found. Render it from .env.deploy.example first." >&2
  exit 1
fi

# Read a value out of the env file without exporting the whole file into this shell,
# so a secret cannot leak through `set -x` or into a child process's environment.
read_env() {
  sed -n "s/^$1=//p" "$ENV_FILE" | tail -1
}

require_env() {
  if [ -z "$(read_env "$1")" ]; then
    echo "Error: $1 is empty in $ENV_FILE." >&2
    exit 1
  fi
}

require_env PUBLIC_URL
require_env CLOUDFLARE_TUNNEL_TOKEN
# Without this the build bakes in the localhost fallback, and the site then loads but
# every request fails - a silent break that is hard to trace back to a missing variable.
require_env NEXT_PUBLIC_API_URL

# The api image holds a binary the deploy workflow built for x86-64. Anything else would start
# and die with "exec format error", so it is refused here, where the reason is readable.
if [ -z "${SKIP_ARCH_CHECK:-}" ] && [ "$(uname -m)" != "x86_64" ]; then
  echo "Error: this host is $(uname -m), but the api binary is built for x86_64." >&2
  exit 1
fi
if [ ! -f desktop/server/dist/akidev-server ]; then
  echo "Error: desktop/server/dist/akidev-server is missing: the workflow builds it and uploads it." >&2
  exit 1
fi

PUBLIC_URL_VALUE="$(read_env PUBLIC_URL)"
# Derived from the API origin by default, and only used for the summary line.
FRONTEND_URL_VALUE="$(read_env FRONTEND_URL)"
if [ -z "$FRONTEND_URL_VALUE" ]; then
  FRONTEND_URL_VALUE="$(printf '%s' "$PUBLIC_URL_VALUE" | sed 's|//api\.|//|')"
fi

# The images must be tagged before compose is invoked, because compose interpolates
# ${API_IMAGE} and ${FRONTEND_IMAGE} out of the environment.
export API_IMAGE="konflix-api:$IMAGE_TAG"
# One image serves every hostname, so there is no per-site build and no second tag.
export FRONTEND_IMAGE="konflix-frontend:$IMAGE_TAG"
export NEXT_PUBLIC_API_URL="$(read_env NEXT_PUBLIC_API_URL)"
export NEXT_PUBLIC_POSTHOG_TOKEN="$(read_env NEXT_PUBLIC_POSTHOG_TOKEN)"

compose() {
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

# Reports the health of one service as Docker sees it, without needing a shell in the
# container or a published port.
service_health() {
  docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' \
    "$1" 2>/dev/null || echo "missing"
}

wait_for_health() {
  local container="$1" label="$2" deadline=$((SECONDS + HEALTH_TIMEOUT))
  while :; do
    case "$(service_health "$container")" in
      healthy) echo "    $label healthy"; return 0 ;;
      unhealthy)
        echo "Error: $label reported unhealthy." >&2
        docker logs --tail 60 "$container" >&2
        return 1 ;;
    esac
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "Error: $label did not become healthy within ${HEALTH_TIMEOUT}s." >&2
      docker logs --tail 60 "$container" >&2
      return 1
    fi
    sleep 3
  done
}

echo "==> Deploying $IMAGE_TAG"
echo "    api image      $API_IMAGE"
echo "    frontend image $FRONTEND_IMAGE (all sites)"
echo "    api origin     $PUBLIC_URL_VALUE"

if [ -n "${SKIP_BUILD:-}" ]; then
  echo "==> Skipping build (SKIP_BUILD is set)"
else
  echo "==> Building images"
  # The frontend build needs the API address and the site name baked in, both of which
  # are build arguments rather than runtime variables. The api image only unpacks the binary.
  compose build ${PULL:+--pull} api frontend
fi

echo "==> Starting the stack"
# --remove-orphans drops containers from a service that no longer exists, which is how
# a renamed or deleted service stops running instead of lingering.
compose up -d --remove-orphans

# A changed Caddyfile is picked up without a restart, so no connection is dropped.
compose exec -T caddy caddy reload --config /etc/caddy/Caddyfile

echo "==> Waiting for the containers to report healthy"
# The api and frontend ports are only reachable from inside the stack. Caddy is the one thing that
# publishes ports.
wait_for_health konflix-api-1 api
wait_for_health konflix-frontend-1 frontend
wait_for_health konflix-caddy-1 caddy

# Reports what is actually configured, which is the difference between "the process
# started" and "it can do its job". Run inside the container so the host needs no node.
echo "==> Health"
# Reports what is actually configured, which is the difference between "the process started"
# and "it can do its job". Read from the frontend container, which has node and sits on the same
# network as the api; the api image has no shell tools beyond its own binary.
docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" exec -T frontend node -e "
  fetch('http://api:9000/health')
    .then((r) => r.json())
    .then((h) => {
      console.log('    version:        ' + h.version)
      console.log('    stream sources: ' + (h.sources || []).join(', '))
      console.log('    ffmpeg:         ' + h.ffmpeg)
      const debrid = h.debrid || {}
      console.log('    debrid:         ' + (debrid.configured ? (debrid.reachable ? 'ok' : 'UNREACHABLE') : 'not configured'))
    })
    .catch((error) => {
      console.error('    could not read /health: ' + error.message)
      process.exit(1)
    })
"

echo "==> Retiring images from earlier deploys"
# Only this project's images, and only those not tagged with the revision just built.
# A bare `docker image prune` would also remove the node base images that the next
# build needs, forcing a re-pull every time. The currently running images are still
# referenced by their containers, so a removal that races with a viewer is refused
# rather than fatal, which is why the failure is ignored.
docker image ls --format '{{.Repository}}:{{.Tag}}' \
  | grep -E '^konflix-(api|frontend):' \
  | grep -v ":${IMAGE_TAG}\$" \
  | xargs -r docker image rm >/dev/null 2>&1 || true

# Derived from the API origin ("https://api.example.com" -> "example.com") so this
# works for any zone without another variable to keep in sync.
ZONE_VALUE="$(printf '%s' "$PUBLIC_URL_VALUE" | sed -E 's|^https?://||; s|/.*$||; s|^api\.||')"

echo
echo "Deployed $IMAGE_TAG."
echo "  site        https://$ZONE_VALUE"
echo "  api         $PUBLIC_URL_VALUE"
if [ -n "$(read_env STREAM_URL)" ]; then
  echo "  video       $(read_env STREAM_URL)"
fi
