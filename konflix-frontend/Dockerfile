# Konflix frontend image.
#
# The build context is the repository root, not this directory, because the frontend
# is one workspace of a monorepo and installs from the shared lockfile.

# --- build ----------------------------------------------------------------------
FROM node:22-bookworm-slim AS build

WORKDIR /app

COPY package.json package-lock.json ./
COPY anime-server/package.json ./anime-server/
COPY konflix-frontend/package.json ./konflix-frontend/

# The build needs devDependencies (Next, Tailwind, PostCSS), so --omit=dev is
# deliberately absent here.
RUN npm ci --workspace=konflix --include=dev --no-audit --no-fund

COPY konflix-frontend ./konflix-frontend

# Next inlines NEXT_PUBLIC_* into the browser bundle at build time, so this has to be
# present now. Changing the API address therefore requires a rebuild, not a restart.
ARG NEXT_PUBLIC_GRAPHQL_URI
ENV NEXT_PUBLIC_GRAPHQL_URI=$NEXT_PUBLIC_GRAPHQL_URI

RUN npm run build --workspace=konflix

# --- runtime --------------------------------------------------------------------
FROM node:22-bookworm-slim

WORKDIR /app

COPY package.json package-lock.json ./
COPY anime-server/package.json ./anime-server/
COPY konflix-frontend/package.json ./konflix-frontend/

# The same --ignore-scripts reason as the API image: the workspace tree contains
# node-datachannel (via webtorrent), whose install script always fails. Only the
# frontend's own dependencies are needed here, and none of them have install scripts.
RUN npm ci --workspace=konflix --omit=dev --ignore-scripts --no-audit --no-fund

COPY --from=build /app/konflix-frontend/.next ./konflix-frontend/.next
COPY --from=build /app/konflix-frontend/public ./konflix-frontend/public
COPY --from=build /app/konflix-frontend/next.config.js ./konflix-frontend/
COPY --from=build /app/konflix-frontend/postcss.config.js ./konflix-frontend/
COPY --from=build /app/konflix-frontend/tailwind.config.js ./konflix-frontend/

WORKDIR /app/konflix-frontend

ENV NODE_ENV=production

# Nothing here needs to write to disk or to be root.
USER node

EXPOSE 3000

# The host and port come from the package.json start script so there is one place
# that defines them.
CMD ["npm", "start"]
