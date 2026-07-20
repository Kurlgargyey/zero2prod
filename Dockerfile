FROM rust:latest AS builder
WORKDIR /app
RUN apt update && apt install lld clang -y
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release

FROM debian:stable-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
  && apt-get install -y --no-install-recommends openssl ca-certificates \
  # Clean up
  && apt-get autoremove -y \
  && apt-get clean -y \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/zero2prod zero2prod
COPY configuration configuration
# set environment to production
ENV APP_ENVIRONMENT=production
# start service
ENTRYPOINT ["./zero2prod"]
