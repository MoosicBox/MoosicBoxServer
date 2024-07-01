# Builder
FROM rust:1.79-bookworm as builder
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cat Cargo.toml | \
    tr '\n' '\r' | \
    sed -E "s/members = \[[^]]+\]/members = [\r\
    \"packages\/assert\",\r\
    \"packages\/database\",\r\
    \"packages\/env_utils\",\r\
    \"packages\/json_utils\",\r\
    \"packages\/middleware\",\r\
    \"packages\/tunnel\",\r\
    \"packages\/tunnel_server\",\r\
]/" | tr '\r' '\n' \
    > Cargo2.toml && \
    mv Cargo2.toml Cargo.toml

COPY packages/assert/Cargo.toml packages/assert/Cargo.toml
COPY packages/database/Cargo.toml packages/database/Cargo.toml
COPY packages/env_utils/Cargo.toml packages/env_utils/Cargo.toml
COPY packages/json_utils/Cargo.toml packages/json_utils/Cargo.toml
COPY packages/middleware/Cargo.toml packages/middleware/Cargo.toml
COPY packages/tunnel/Cargo.toml packages/tunnel/Cargo.toml
COPY packages/tunnel_server/Cargo.toml packages/tunnel_server/Cargo.toml

RUN touch temp_lib.rs

RUN for file in $(\
    for file in packages/*/Cargo.toml; \
      do printf "$file\n"; \
    done | grep -E "^(\
packages/assert|\
packages/database|\
packages/env_utils|\
packages/json_utils|\
packages/middleware|\
packages/tunnel|\
packages/tunnel_server|\
)/Cargo.toml$"); \
    do printf "\n\n[lib]\npath=\"../../temp_lib.rs\"" >> "$file"; \
  done

RUN mkdir packages/tunnel_server/src && \
  echo 'fn main() {}' >packages/tunnel_server/src/main.rs

ARG TUNNEL_ACCESS_TOKEN
ENV TUNNEL_ACCESS_TOKEN=${TUNNEL_ACCESS_TOKEN}
ENV RUST_LOG=info,moosicbox=trace
RUN cargo build --package moosicbox_tunnel_server --release --no-default-features --features postgres-raw,postgres-native-tls

COPY packages packages

RUN rm target/release/deps/moosicbox*
RUN cargo build --package moosicbox_tunnel_server --release --no-default-features --features postgres-raw,postgres-native-tls

# Final
FROM debian:bookworm-slim

RUN echo 'Acquire::http::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy && \
  echo 'Acquire::ftp::Timeout "10";' >>/etc/apt/apt.conf.d/httpproxy
RUN apt-get update && apt-get install -y ca-certificates curl

COPY --from=builder /app/target/release/moosicbox_tunnel_server /
EXPOSE 8004
ENV RUST_LOG=info,moosicbox=trace
ENV MAX_THREADS=64
ENV ACTIX_WORKERS=32
ARG AWS_ACCESS_KEY_ID
ENV AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID}
ARG AWS_SECRET_ACCESS_KEY
ENV AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY}
CMD ["./moosicbox_tunnel_server", "8004"]
