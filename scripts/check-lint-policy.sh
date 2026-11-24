#!/bin/sh
set -eu

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

lint_table() {
    awk -v table="[workspace.lints.$1]" '
        $0 == table { found = 1; next }
        /^\[/ { found = 0 }
        found { print }
    ' Cargo.toml
}

require_level() {
    lint_table "$1" | grep -qxF "$2 = \"$3\"" \
        || fail "Cargo.toml must set the $1 lint $2 to $3."
}

require_level rust unsafe_code forbid
for lint in \
    exit expect_used get_unwrap mem_forget panic todo unimplemented \
    unreachable unwrap_used; do
    require_level clippy "$lint" forbid
done
for lint in await_holding_lock let_underscore_future let_underscore_must_use; do
    require_level clippy "$lint" deny
done

for exemption in dbg expect indexing-slicing panic print unwrap; do
    grep -qxF "allow-$exemption-in-tests = true" clippy.toml \
        || fail "clippy.toml must set allow-$exemption-in-tests to true."
done

channel=$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)
[ -n "$channel" ] || fail 'rust-toolchain.toml must name one channel.'
for manifest in Cargo.toml crates/*/Cargo.toml; do
    awk '/^\[lints\]$/ { getline; if ($0 == "workspace = true") found = 1 } END { exit !found }' \
        "$manifest" || fail "$manifest must use the workspace lint table."
    grep -qxF "rust-version = \"$channel\"" "$manifest" \
        || fail "$manifest must name rust-version $channel, the rust-toolchain.toml channel."
done
