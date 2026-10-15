# Akidev frontend image.
#
# One image serves every hostname: the anime site, the TV site, and the service picker on
# the bare zone. Which one a request gets is decided from its Host header at runtime (see
# src/middleware.ts), so there is nothing per-site to bake in and only one build to make.
#
# The build context is the repository root, not this directory, because the frontend
# is one workspace of a monorepo and installs from the shared lockfile.
#
# Next is built with `output: "standalone"`, which emits a self-contained server
# bundle. The runtime stage therefore copies that bundle instead of the workspace
# tree, so it carries no devDependencies, no source, and no test files.

# --- build ----------------------------------------------------------------------
FROM node:22-bookworm-slim AS build

WORKDIR /app

COPY package.json package-lock.json ./
COPY akidev-api/package.json ./akidev-api/
COPY akidev-frontend/package.json ./akidev-frontend/

# The build needs devDependencies (Next, Tailwind, TypeScript, ESLint), so
# --omit=dev is deliberately absent here. --ignore-scripts for the same reason as
# the API image: the workspace tree contains node-datachannel (via webtorrent),
# whose install script always fails. None of the frontend's own dependencies have
# install scripts.
RUN npm ci --workspace=akidev --include=dev --ignore-scripts --no-audit --no-fund

COPY akidev-frontend ./akidev-frontend

# Next inlines NEXT_PUBLIC_* into the browser bundle at build time, so both have to be
# present now. Changing either therefore requires a rebuild, not a restart.
#
# NEXT_PUBLIC_SITE_DOMAIN is the zone the sites are served from. One value for the whole
# deployment: which site a request belongs to is decided from the request's Host header
# at runtime, not from this build argument, so one image serves every hostname.
ARG NEXT_PUBLIC_GRAPHQL_URI
ARG NEXT_PUBLIC_SITE_DOMAIN=akidev.live
ENV NEXT_PUBLIC_GRAPHQL_URI=$NEXT_PUBLIC_GRAPHQL_URI
ENV NEXT_PUBLIC_SITE_DOMAIN=$NEXT_PUBLIC_SITE_DOMAIN

# An empty value produces a build that succeeds and a browser bundle that calls the
# viewer's own localhost, which looks like a broken API rather than a missing build
# argument. Failing here makes the mistake visible where it is made.
RUN test -n "$NEXT_PUBLIC_GRAPHQL_URI" || (echo "NEXT_PUBLIC_GRAPHQL_URI build argument is required" >&2; exit 1)

RUN npm run build --workspace=akidev

# --- runtime --------------------------------------------------------------------
FROM node:22-bookworm-slim

WORKDIR /app

ENV NODE_ENV=production
# PORT is deliberately not set here. It is a runtime concern and compose supplies it;
# server.js reads it. One container is all that is needed now.
ENV HOSTNAME=0.0.0.0

# The standalone output is rooted at the workspace directory, so it is unpacked into
# the same relative path and run from there. server.js resolves its own files
# relative to itself, which is why the nesting is preserved.
COPY --from=build /app/akidev-frontend/.next/standalone ./
COPY --from=build /app/akidev-frontend/.next/static ./akidev-frontend/.next/static
COPY --from=build /app/akidev-frontend/public ./akidev-frontend/public

WORKDIR /app/akidev-frontend

# next/image writes optimised images to <distDir>/cache/images. COPY leaves those
# directories owned by root, so a container running as `node` could not write to them
# and every poster would fail to optimise. The cache directory is created and handed
# over before privileges are dropped.
RUN mkdir -p .next/cache && chown -R node:node .next

# Nothing here needs to write to disk or to be root.
USER node

EXPOSE 3000

CMD ["node", "server.js"]
