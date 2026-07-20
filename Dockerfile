# Use latest stable rust image
FROM rust:latest
# create app directory
WORKDIR /app
# install os dependencies
RUN apt update && apt install lld clang -y
# copy source into container
COPY . .
# put sqlx into offline mode
ENV SQLX_OFFLINE=true
# build app on release profile
RUN cargo build --release
# start service
ENTRYPOINT ["./target/release/zero2prod"]

