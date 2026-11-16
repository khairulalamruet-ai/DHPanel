#!/usr/bin/env bash
# Builds the release programs inside Ubuntu 22.04 (glibc 2.35), so that they run on 22.04 and
# on everything newer (Ubuntu 24.04, Debian 12), which a build on a newer system does not:
# a program carries the glibc version of the machine it was linked on. Run on the build box:
#   deploy/dev/remote.sh sh 'deploy/dev/build-release-jammy.sh'
# The programs are left in ~/release-jammy/bin (a folder `install.sh --bin-dir` takes).
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT="$HOME/release-jammy"; rm -rf "$OUT"; mkdir -p "$OUT/bin"
docker volume create dh-jammy-target > /dev/null
docker run --rm \
    -v "$PWD:/src:ro" -v dh-jammy-target:/target -v "$OUT/bin:/out" \
    -v "$HOME/.cargo/registry:/root/.cargo/registry" \
    -e CARGO_TARGET_DIR=/target -e SQLX_OFFLINE=true -e CARGO_TERM_COLOR=never -e DEBIAN_FRONTEND=noninteractive \
    ubuntu:22.04 bash -euc '
        apt-get update -qq && apt-get install -y -qq --no-install-recommends ca-certificates curl build-essential pkg-config > /dev/null
        curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$(sed -n "s/^channel *= *\"\(.*\)\"/\1/p" /src/rust-toolchain.toml)" > /dev/null
        . /root/.cargo/env
        cd /src
        cargo build --release --locked -p dhpanel-control-api -p dhpanel-dhctl -p dhpanel-node-agent -p dhpanel-priv-helper
        for b in dhpanel-control-api dhctl dhpanel-agent dhpanel-priv-helper; do cp "/target/release/$b" /out/; done
        echo "glibc needed:"; for b in /out/*; do printf "  %-24s %s\n" "$(basename $b)" "$(objdump -T $b | grep -o "GLIBC_[0-9.]*" | sort -V | tail -1)"; done
    '
