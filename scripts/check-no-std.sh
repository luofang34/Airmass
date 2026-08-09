#!/bin/sh
set -eu

for target in \
    thumbv6m-none-eabi \
    thumbv8m.main-none-eabihf \
    riscv32imac-unknown-none-elf \
    wasm32-unknown-unknown; do
    cargo check --locked \
        -p airmass-aero-link \
        -p airmass-core \
        -p airmass \
        --no-default-features \
        --target "$target"
done
