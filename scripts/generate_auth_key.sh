#!/bin/bash
set -e

if ! command -v openssl &> /dev/null; then
    echo "Error: openssl is not installed"
    echo "Please install openssl to generate authentication keys"
    exit 1
fi

echo "Generating authentication key..."
AUTH_KEY=$(openssl rand -hex 32)

echo ""
echo "==================================="
echo "Authentication Key Generated"
echo "==================================="
echo ""
echo "$AUTH_KEY"
echo ""
echo "Add this to both server and client .env files:"
echo "  AUTH_SECRET_KEY=$AUTH_KEY"
echo ""
