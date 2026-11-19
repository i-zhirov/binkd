#!/usr/bin/env bash
# Package one release binary for a Unix target.
#
# Usage: ci/package-release.sh <tag> <target> <binary> [output-dir]
set -euo pipefail

tag=${1:?the release tag is required}
target=${2:?the target triple is required}
binary=${3:?the binary path is required}
output=${4:-dist}
name="rbinkd-${tag}-${target}"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

mkdir -p "${stage}/${name}" "$output"
cp "$binary" "${stage}/${name}/rbinkd"
cp README.md COPYING rbinkd.8 binkd.conf binkd.cfg "${stage}/${name}/"
tar -czf "${output}/${name}.tar.gz" -C "$stage" "$name"
