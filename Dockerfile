# syntax=docker/dockerfile:1.7

FROM clux/muslrust:stable AS chef
USER root
RUN cargo install --locked cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --target x86_64-unknown-linux-musl --bin caliborn

FROM alpine:3.20 AS runtime
RUN apk add --no-cache ca-certificates tini \
    && addgroup -S caliborn && adduser -S caliborn -G caliborn
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/caliborn /usr/local/bin/caliborn
USER caliborn
WORKDIR /home/caliborn
ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/caliborn"]
