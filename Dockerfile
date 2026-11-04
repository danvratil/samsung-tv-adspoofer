FROM rust:1-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home adspoofer \
    && mkdir -p /data/files \
    && chown -R adspoofer:adspoofer /data

COPY --from=builder /build/target/release/samsung-tv-adspoofer /usr/local/bin/
USER adspoofer
WORKDIR /data
EXPOSE 8080
ENTRYPOINT ["samsung-tv-adspoofer"]
CMD ["--listen", "0.0.0.0:8080", "--database", "/data/database.sqlite", "--storage", "/data/files"]
