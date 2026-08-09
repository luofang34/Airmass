#!/bin/sh
set -eu

if ! rg --quiet \
    '^aero-link = \{ path = "\.\./aero-link", default-features = false \}$' \
    Cargo.toml; then
    printf '%s\n' 'aero-link must use the portable dependency form' >&2
    exit 1
fi

aero_link_ref=$(cat ci/aero-link.ref)
if [ "${#aero_link_ref}" -ne 40 ] \
    || ! printf '%s\n' "$aero_link_ref" | rg --quiet '^[0-9a-f]{40}$'; then
    printf '%s\n' 'ci/aero-link.ref must contain one full commit SHA' >&2
    exit 1
fi

traffic_pattern='([.]traffic[[:space:]]*[(]|ReceptionEvent::traffic)'
if rg --line-number "$traffic_pattern" src tests --glob '*.rs'; then
    printf '%s\n' 'Airmass must not call the traffic projection' >&2
    exit 1
fi

if rg --line-number '\bFisBAssembler\b' src --glob '*.rs'; then
    printf '%s\n' 'Airmass must accept complete products and not assemble segments' >&2
    exit 1
fi

dependency_tree=$(cargo tree --edges normal --no-default-features --prefix none)
if printf '%s\n' "$dependency_tree" | rg --quiet \
    '^(aero-link-usb|nusb|rusb|libusb|serialport) '; then
    printf '%s\n' 'Airmass must not enable a radio or USB dependency' >&2
    exit 1
fi
