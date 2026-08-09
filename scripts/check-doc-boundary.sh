#!/bin/sh
set -eu

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

if grep -nE '(^|[^[:alnum:]_])Airspace([^[:alnum:]_]|$)' \
    README.md AGENTS.md docs/roadmap.md; then
    fail 'Airmass documentation must use AeronauticalUpdates.'
fi

if grep -nE \
    'FisBOwner|route_fis_b_product|routing table assigns|exactly one application owner' \
    README.md AGENTS.md docs/roadmap.md; then
    fail 'Airmass documentation must not use application-owner routing.'
fi

for document in README.md AGENTS.md docs/roadmap.md; do
    grep -q 'AeronauticalUpdates' "$document" \
        || fail "$document must name AeronauticalUpdates."
    grep -q 'WeatherIngress' "$document" \
        || fail "$document must name WeatherIngress."
    grep -q 'airmass-aero-link' "$document" \
        || fail "$document must name airmass-aero-link."
    grep -q 'aero-link/issues/62' "$document" \
        || fail "$document must link aero-link issue 62."
done
