#!/bin/sh
set -eu

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

aero_link_ref=$(cat ci/aero-link.ref)
if [ "${#aero_link_ref}" -ne 40 ] \
    || ! printf '%s\n' "$aero_link_ref" | grep -qE '^[0-9a-f]{40}$'; then
    fail 'ci/aero-link.ref must contain one full commit SHA.'
fi

if grep -RInE \
    'aero[_-]link|CompleteFisBProduct|ReceptionEvent|FisBAssembler' \
    crates/airmass-core/src crates/airmass-core/tests 2>/dev/null; then
    fail 'airmass-core must not name a link-service type.'
fi

traffic_pattern='[.]traffic[[:space:]]*[(]|ReceptionEvent::traffic'
if grep -RInE "$traffic_pattern" src crates 2>/dev/null; then
    fail 'Airmass must not call the traffic projection.'
fi

if grep -RIn 'CompleteFisBProduct' src crates 2>/dev/null; then
    fail 'Airmass must consume classified exchange records, not complete products.'
fi

if ! grep -qE \
    '^aero-link = \{ path = "../aero-link", default-features = false \}$' \
    Cargo.toml; then
    fail 'the aero-link dependency must disable default features.'
fi

if grep -nE 'aero[_-]link|CompleteFisBProduct|ReceptionEvent' \
    crates/airmass-core/Cargo.toml; then
    fail 'the airmass-core manifest must not name a link-service dependency.'
fi

find src crates -type f -name '*.rs' -exec sh -c '
    for file do
        lines=$(wc -l < "$file")
        if [ "$lines" -gt 500 ]; then
            printf "%s has %s lines; the limit is 500.\n" "$file" "$lines" >&2
            exit 1
        fi
        case "$file" in
            */lib.rs)
                if [ "$lines" -ge 100 ]; then
                    printf "%s has %s lines; lib.rs must be under 100.\n" \
                        "$file" "$lines" >&2
                    exit 1
                fi
                ;;
        esac
    done
' sh {} +

core_tree=$(cargo tree --locked -p airmass-core --no-default-features --prefix none)
if printf '%s\n' "$core_tree" | grep -qE '^(aero-link|airmass-aero-link) '; then
    fail 'airmass-core must not depend on a source adapter or link service.'
fi

root_tree=$(cargo tree --locked -p airmass --no-default-features --prefix none)
if printf '%s\n' "$root_tree" | grep -qE '^(aero-link|airmass-aero-link) '; then
    fail 'the default Airmass build must not include a source adapter.'
fi

workspace_tree=$(cargo tree --locked --workspace --all-features --prefix none)
if printf '%s\n' "$workspace_tree" | grep -qE \
    '^(nusb|rusb|libusb|serialport|tokio|async-std) '; then
    fail 'the Airmass core workspace must stay portable and synchronous.'
fi
