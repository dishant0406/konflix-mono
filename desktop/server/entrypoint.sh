#!/bin/sh
# Starts the server as the unprivileged user.
#
# It begins as root only to hand the data directory over: a named volume keeps the owner it was
# first created with, and the volume this replaced was made by the previous image's user, so the
# server could not have written to it. Everything in it is scratch data.
set -e
mkdir -p "${CACHE_DIR:-/data}"
chown -R akidev:akidev "${CACHE_DIR:-/data}" 2>/dev/null || true
# The DHT keeps its routing table under $HOME, which setpriv would leave pointing at root.
export HOME="${CACHE_DIR:-/data}"
exec setpriv --reuid=akidev --regid=akidev --init-groups /usr/local/bin/akidev-server "$@"
