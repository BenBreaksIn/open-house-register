FROM rust:1.94-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY api ./api
COPY migrations ./migrations
COPY web ./web
RUN cargo build --locked --release --bin open-house-register

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* && useradd --system --uid 10001 houseworks
COPY --from=builder /build/target/release/open-house-register /usr/local/bin/open-house-register
USER houseworks
ENV HOST=0.0.0.0 PORT=3030
EXPOSE 3030
CMD ["open-house-register"]
