#!/bin/bash
set -e

SERVER_IP="${1:-127.0.0.1}"
CERT_DIR="${2:-./certs}"

echo "==================================="
echo "TradeTerminal Certificate Generator"
echo "==================================="
echo ""
echo "Server IP: $SERVER_IP"
echo "Output directory: $CERT_DIR"
echo ""

cd "$(dirname "$0")/.."

cargo build --release --bin generate-cert

./target/release/generate-cert --ip "$SERVER_IP" --output-dir "$CERT_DIR"

echo ""
echo "Done! Certificate files are in $CERT_DIR/"
