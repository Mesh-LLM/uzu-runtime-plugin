#!/bin/bash
# Thin adapters around git, Cargo, codesign and tar; no model execution.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
source_dir=$(cd "${1:?provide a clean pinned Uzu checkout}" && pwd)
revision=0d60981a899c79fc1c29ab3ca1c5aeff937affff
[[ $(uname -s) == Darwin && $(uname -m) == arm64 ]]
[[ $(git -C "$source_dir" rev-parse HEAD) == "$revision" ]]
[[ -z $(git -C "$source_dir" status --porcelain) ]]
rustc_path=$(rustup which --toolchain 1.99.0 rustc)
toolchain_bin=$(dirname "$rustc_path")
export PATH="$toolchain_bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=26.4
# Upstream uses Metal 4 with a 26.4 minimum; fail before expensive compilation.
xcrun -sdk macosx metal --version
sdk=$(xcrun -sdk macosx --show-sdk-version)
major=${sdk%%.*}
minor=${sdk#*.}; minor=${minor%%.*}
(( major > 26 || (major == 26 && minor >= 4) ))
cd "$root"
mkdir -p dist
stage=$(mktemp -d "$root/dist/package.XXXXXX")
cleanup() {
    rm -rf "$stage"
    git -C "$source_dir" apply --reverse "$root/uzu-server.patch"
    rustup run 1.99.0 cargo clean --manifest-path "$source_dir/Cargo.toml"
    rustup run 1.99.0 cargo clean --manifest-path "$root/Cargo.toml"
}
git -C "$source_dir" apply --check "$root/uzu-server.patch"
git -C "$source_dir" apply "$root/uzu-server.patch"
trap cleanup EXIT
(cd "$source_dir" && RUSTC_BOOTSTRAP=1 rustup run 1.99.0 cargo build --locked --release -p cli -j 2)
rustup run 1.99.0 cargo build --locked --release -j 2
bundle="$stage/uzu-runtime-plugin"
mkdir -p "$bundle"
cp target/release/uzu-plugin "$bundle/uzu-runtime-plugin"
cp "$source_dir/target/release/cli" "$bundle/uzu-server"
sed 's/"uzu-plugin"/"uzu-runtime-plugin"/g' plugin.toml > "$bundle/plugin.toml"
cp README.md LICENSE UZU_LICENSE uzu-server.patch "$bundle/"
printf '%s\n' "$revision" > "$bundle/UPSTREAM_REVISION"
for binary in uzu-runtime-plugin uzu-server; do
    codesign --force --sign - "$bundle/$binary"
    codesign --verify --strict "$bundle/$binary"
    otool -L "$bundle/$binary" | tail -n +2 > "$bundle/$binary.libraries.txt"
    # Reject accidental Homebrew/build-machine dylib dependencies.
    if grep -E '/opt/|/Users/|/usr/local/' "$bundle/$binary.libraries.txt"; then exit 1; fi
done
(cd "$bundle" && shasum -a 256 uzu-runtime-plugin uzu-server > BINARIES.sha256)
archive=uzu-runtime-plugin-aarch64-apple-darwin.tar.gz
tar -czf "dist/$archive" -C "$stage" uzu-runtime-plugin
(cd dist && shasum -a 256 "$archive" > "$archive.sha256")
bash scripts/smoke-package.sh "dist/$archive"
