# 1. Build Stage
FROM ubuntu:24.04 AS builder

ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /app

# Installeer build dependencies, GCC, OpenSSL en curl
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Installeer actuele Rust toolchain via rustup
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

# Manifests kopiëren
COPY Cargo.toml Cargo.lock* ./

# Dummy build om dependencies te cachen
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true
RUN rm -rf src

# Echte code kopiëren en bouwen
COPY src ./src
RUN touch src/main.rs && cargo build --release --bin ffkwijt-api

# 2. Runtime Stage
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

# Binary overzetten
COPY --from=builder /app/target/release/ffkwijt-api /app/ffkwijt-api

# Eventuele shared libraries meenemen en ldconfig draaien
COPY --from=builder /app/target/release/*.so* /usr/local/lib/
RUN ldconfig || true

# Statische bestanden en modellen meenemen
COPY models ./models
COPY public ./public

ENV PORT=3000
EXPOSE 3000

CMD ["/app/ffkwijt-api"]