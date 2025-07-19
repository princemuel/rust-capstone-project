#!/usr/bin/env bash

cd "$(dirname "$0")" || exit

cargo run --release
