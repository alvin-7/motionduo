#!/bin/sh
set -eu
# Run from the repository root. GO may name an alternate Go 1.27.3+ toolchain.
GO="${GO:-go}"
profile=debug
if [ "${1:-}" = --release ]; then
    profile=release
    cargo build --workspace --release --locked
else
    cargo build --workspace --locked
fi
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || { echo 'Cannot read workspace version' >&2; exit 1; }
"$GO" -C mousehop-mygo build -mod=readonly -trimpath -ldflags "-X main.version=$version" -o "$(pwd)/target/$profile/motionduo-ui" .
printf 'Built target/%s/motionduo and target/%s/motionduo-ui\n' "$profile" "$profile"
