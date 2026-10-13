# Konflix API image.
#
# The build context is the repository root, not this directory, because the API is
# one workspace of a monorepo and installs from the shared lockfile.

# --- dependencies ---------------------------------------------------------------
FROM node:22-bookworm AS deps

WORKDIR /app

COPY package.json package-lock.json ./
COPY anime-server/package.json ./anime-server/
COPY konflix-frontend/package.json ./konflix-frontend/

# build-essential and python3 are needed by the native modules that are rebuilt
# below. They stay in this stage only.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential python3 ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

# Only the API workspace. The frontend is a separate image, and installing its
# dependencies here would more than double this one for nothing.
#
# --ignore-scripts because one dependency's install script cannot succeed in any
# container, and it fails the whole install rather than degrading:
#
#   webtorrent -> @thaunknown/simple-peer -> webrtc-polyfill -> node-datachannel
#
# node-datachannel's install is `prebuild-install -r napi || (npm install && npm run
# _prebuild)`. The first command finds no usable prebuild (it reports "does not
# support N-API version undefined" and exits non-zero), so it falls back to compiling
# from source - but the npm tarball contains no `deps/libdatachannel`, because that is
# a git submodule that is not published. The fallback therefore always fails, with
# `TypeError: expected first argument to be an array`, on every platform and
# architecture. Verified on linux/arm64 and linux/amd64, and on the host.
#
# The package does publish a working prebuilt binary as a release asset, so it is
# installed below instead of being built. The remaining native modules are rebuilt
# afterwards, once the failing script is out of the way.
RUN npm ci --workspace=konflix-api --omit=dev --ignore-scripts --no-audit --no-fund

# npm hoists every dependency to the root node_modules when nothing conflicts, so the
# workspace one is normally absent. It is created here so the runtime stage can copy
# it unconditionally: an empty directory is a valid copy source, a missing one is not.
RUN mkdir -p /app/anime-server/node_modules

# Installs the prebuilt node-datachannel binary.
#
# The package directory is derived from `require.resolve` rather than assumed to be
# under the workspace. npm hoists a dependency to the highest node_modules that can
# hold it, and node-datachannel arrives transitively through webtorrent, so it lands
# in the root node_modules - not anime-server/node_modules. Hardcoding the workspace
# path made this step pass only while the layer stayed cached; the first dependency
# change rebuilt it and the build failed on a missing package.json.
#
# `require.resolve("node-datachannel/package.json")` cannot be used because the
# package's "exports" map does not expose it. The entry point is resolved instead and
# walked up until the directory holding package.json is found.
#
# The asset name carries the architecture, and Debian's name for it differs from the
# one the project uses, so it is mapped explicitly. `napi-v8` is the ABI tag used by
# every Node 22 release, which is what this image pins.
RUN set -eux; \
    PKG="$(node -e 'const fs=require("fs"),path=require("path");let d=path.dirname(require.resolve("node-datachannel"));while(!fs.existsSync(path.join(d,"package.json"))){const up=path.dirname(d);if(up===d)throw new Error("node-datachannel package.json not found");d=up;}process.stdout.write(d)')"; \
    VERSION="$(node -p "require('$PKG/package.json').version")"; \
    case "$(dpkg --print-architecture)" in \
      amd64) ARCH=x64 ;; \
      arm64) ARCH=arm64 ;; \
      armhf) ARCH=arm ;; \
      *) echo "Unsupported architecture: $(dpkg --print-architecture)" >&2; exit 1 ;; \
    esac; \
    curl -fsSL --retry 3 -o /tmp/ndc.tar.gz \
      "https://github.com/murat-dogan/node-datachannel/releases/download/v${VERSION}/node-datachannel-v${VERSION}-napi-v8-linux-${ARCH}.tar.gz"; \
    tar -xzf /tmp/ndc.tar.gz -C "$PKG"; \
    rm /tmp/ndc.tar.gz; \
    test -f "$PKG/build/Release/node_datachannel.node"

# Rebuilds the native modules whose scripts are safe, so uTP peer connections work.
# webtorrent treats utp-native as optional and falls back to TCP, but uTP is a large
# share of real peers, so it is worth the compile.
RUN npm rebuild utp-native bufferutil utf-8-validate --no-audit --no-fund

# --- runtime --------------------------------------------------------------------
FROM node:22-bookworm-slim

# ffmpeg provides ffprobe, which the stream layer uses to read a container header and
# to remux when a release is not directly playable.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY package.json package-lock.json ./
COPY anime-server/package.json ./anime-server/
COPY konflix-frontend/package.json ./konflix-frontend/
COPY --from=deps /app/node_modules ./node_modules
# The workspace node_modules is usually absent, because npm hoists every dependency
# to the root node_modules when nothing conflicts - which is the case for the whole
# dependency tree here. The deps stage creates the directory empty so this copy is
# valid either way, and so a dependency that could not be hoisted would still be
# carried across instead of failing at runtime with MODULE_NOT_FOUND.
COPY --from=deps /app/anime-server/node_modules ./anime-server/node_modules

# Copied file by file rather than as the whole directory, so the test suite and any
# stray local file do not end up in the image.
COPY anime-server/index.js ./anime-server/
COPY anime-server/graphql ./anime-server/graphql
COPY anime-server/lib ./anime-server/lib
COPY anime-server/package.json ./anime-server/

WORKDIR /app/anime-server

ENV NODE_ENV=production

# The torrent cache is written here and mounted as a volume. It is created and owned
# by the unprivileged user the container runs as, so the volume inherits that
# ownership when Docker initialises it.
RUN mkdir -p /data && chown -R node:node /data

# A viewer's connection is one file descriptor each, and the process has no reason to
# run as root.
USER node

EXPOSE 9000

# `node index.js` directly rather than `npm start`: the start script passes
# --env-file-if-exists=.env, and in this image the environment comes from the compose
# service definition instead of a file.
CMD ["node", "index.js"]
