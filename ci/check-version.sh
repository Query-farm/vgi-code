#!/usr/bin/env bash
set -euo pipefail
TAG="${1:?usage: check-version.sh <release-tag>}"; TAG="${TAG#v}"
HERE="$(cd "$(dirname "$0")" && pwd)"; CARGO_TOML="$HERE/../Cargo.toml"
VERSION="$(awk '/^\[(workspace\.)?package\]/{p=1;next}/^\[/{p=0}p&&/^[[:space:]]*version[[:space:]]*=/{if(match($0,/"[^"]+"/)){print substr($0,RSTART+1,RLENGTH-2);exit}}' "$CARGO_TOML")"
[ -n "$VERSION" ] || { echo "::error::no version in $CARGO_TOML" >&2; exit 1; }
[ "$TAG" = "$VERSION" ] || { echo "::error::tag ($TAG) != Cargo version ($VERSION)" >&2; exit 1; }
echo "version OK: $VERSION matches release tag"
