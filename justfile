set dotenv-load

default:
    @just --list

# Run server locally
server:
    cargo run --bin trade-server

# Run client
client:
    BINDGEN_EXTRA_CLANG_ARGS="--target=aarch64-apple-darwin -isysroot $(xcrun --show-sdk-path)" cargo run --bin trade-client

# Run all tests
test:
    cargo test

# Build release
build:
    cargo build --release

# Build and run server in Docker
docker-up:
    docker-compose up -d

# Stop Docker
docker-down:
    docker-compose down

# View Docker logs
docker-logs:
    docker-compose logs -f

# Check code
check:
    cargo check

# Format code
fmt:
    cargo fmt

# Clippy lint
lint:
    cargo clippy

# Clean build artifacts
clean:
    cargo clean

# Build server for Linux amd64
build-linux-server-amd64:
    @mkdir -p dist
    docker build --platform linux/amd64 -t trade-server-build-amd64 -f server/Dockerfile .
    @docker rm -f tmp-extract 2>/dev/null || true
    docker create --name tmp-extract trade-server-build-amd64
    docker cp tmp-extract:/usr/local/bin/trade-server ./dist/trade-server-linux-amd64
    docker rm tmp-extract
    @ls -lh dist/trade-server-linux-amd64

# Build server for Linux arm64
build-linux-server-arm64:
    @mkdir -p dist
    docker build --platform linux/arm64 -t trade-server-build-arm64 -f server/Dockerfile .
    @docker rm -f tmp-extract 2>/dev/null || true
    docker create --name tmp-extract trade-server-build-arm64
    docker cp tmp-extract:/usr/local/bin/trade-server ./dist/trade-server-linux-arm64
    docker rm tmp-extract
    @ls -lh dist/trade-server-linux-arm64

# Build server for both architectures
build-linux-server: build-linux-server-amd64 build-linux-server-arm64

# Deploy server to VPS via scp (requires VPS_HOST, VPS_USER in .env)
deploy-server arch="amd64":
    just build-linux-server-{{arch}}
    ssh $VPS_USER@$VPS_HOST "mkdir -p ${VPS_PATH:-/opt/trade-terminal}"
    scp dist/trade-server-linux-{{arch}} $VPS_USER@$VPS_HOST:${VPS_PATH:-/opt/trade-terminal}/trade-server-linux-{{arch}}
    ssh $VPS_USER@$VPS_HOST "chmod +x ${VPS_PATH:-/opt/trade-terminal}/trade-server-linux-{{arch}}"
    @echo "Deployed to $VPS_HOST:${VPS_PATH:-/opt/trade-terminal}/trade-server-linux-{{arch}}"
