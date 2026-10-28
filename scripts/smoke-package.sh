#!/bin/bash
set -euo pipefail
archive=${1:?provide archive}
(cd "$(dirname "$archive")" && shasum -a 256 -c "$(basename "$archive").sha256")
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
tar -xzf "$archive" -C "$stage"
cd "$stage/uzu-runtime-plugin"
shasum -a 256 -c BINARIES.sha256
grep -qx 'name = "uzu-runtime-plugin"' plugin.toml
grep -qx 'command = "uzu-runtime-plugin"' plugin.toml
for binary in uzu-runtime-plugin uzu-server; do
    test -x "$binary"
    codesign --verify --strict "$binary"
    "./$binary" --help >/dev/null
done
# No handshake: must fail without launching/downloading a model.
if env -u MESH_LLM_PLUGIN_ENDPOINT ./uzu-runtime-plugin --model package-smoke >smoke.log 2>&1; then
    echo 'Standalone plugin unexpectedly succeeded' >&2; exit 1
fi
grep -q 'launch through Mesh; no engine started' smoke.log
