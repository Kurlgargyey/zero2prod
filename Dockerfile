FROM rust:latest AS builder
WORKDIR /app
RUN apt update && apt install lld clang -y
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release

FROM rust:slim AS runtime
COPY --from=builder /app/target/release/zero2prod zero2prod
COPY configuration configuration
# set environment to production
ENV APP_ENVIRONMENT=production
# start service
ENTRYPOINT ["./zero2prod"]
