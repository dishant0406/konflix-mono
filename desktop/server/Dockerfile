# The Akidev server image.
#
# It holds a binary built elsewhere: the deploy workflow compiles it inside rust:1-bookworm, the
# same Debian release as this image, and drops it in dist/. Compiling on the server would need
# gigabytes of memory for a few minutes on every deploy, and this way the host only unpacks 20MB.
#
# To build one by hand:
#   docker run --rm -v "$PWD/..:/src" -w /src rust:1-bookworm \
#     cargo build --release --locked -p akidev-server
#   mkdir -p dist && cp ../target/release/akidev-server dist/
#   docker build -t akidev-server .
FROM debian:bookworm-slim

# ffmpeg and ffprobe: the conversion of what a browser cannot play. ca-certificates for the HTTPS
# calls to the catalogues, the indexers and the debrid service.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# An unprivileged user, and a data directory it owns. Docker initialises a named volume from the
# image, so the volume is writable by this user too.
RUN useradd --system --uid 10001 --home-dir /data akidev \
 && mkdir -p /data \
 && chown akidev:akidev /data

COPY dist/akidev-server /usr/local/bin/akidev-server

USER akidev
ENV CACHE_DIR=/data \
    PORT=9000 \
    RUST_LOG=akidev_server=info,akidev_core=info
VOLUME /data
EXPOSE 9000
HEALTHCHECK --interval=30s --timeout=10s --start-period=20s --retries=5 \
  CMD ["akidev-server", "--check"]
ENTRYPOINT ["akidev-server"]
