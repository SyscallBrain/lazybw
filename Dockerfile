# lazybw with the Bitwarden CLI, ready to run:
#
#   docker run -it --rm -v lazybw:/home/lazybw ghcr.io/syscallbrain/lazybw
#
# The volume keeps the bw login, lazybw's config and its state between runs.

# Build a static binary.
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p lazybw \
    && cp target/release/lazybw /lazybw

# Runtime: the official Bitwarden CLI from npm, and lazybw.
FROM node:26-trixie-slim
ARG BW_VERSION=2026.10.1
LABEL org.opencontainers.image.title="lazybw" \
      org.opencontainers.image.description="A keyboard-first terminal UI for the Bitwarden vault" \
      org.opencontainers.image.source="https://github.com/SyscallBrain/lazybw" \
      org.opencontainers.image.documentation="https://syscallbrain.github.io/lazybw/" \
      org.opencontainers.image.licenses="MIT"
RUN npm install --global --omit=dev "@bitwarden/cli@${BW_VERSION}" \
    && npm cache clean --force \
    && useradd --create-home --shell /usr/sbin/nologin lazybw
COPY --from=build /lazybw /usr/local/bin/lazybw
USER lazybw
WORKDIR /home/lazybw
ENV TERM=xterm-256color \
    COLORTERM=truecolor
VOLUME ["/home/lazybw"]
ENTRYPOINT ["lazybw"]
