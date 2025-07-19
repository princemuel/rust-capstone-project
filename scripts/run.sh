#!/usr/bin/env bash

SCRIPT_DIR="$(dirname "$0")"
RUST_PROJECT_DIR="$SCRIPT_DIR/../rust"
cd "$RUST_PROJECT_DIR" || {
    echo "Failed to find rust directory"
    exit 1
}

cargo run --release
