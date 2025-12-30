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
