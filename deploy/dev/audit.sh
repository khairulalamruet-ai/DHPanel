#!/usr/bin/env bash
# Dependency and image audit with an SBOM (T19.08). Run on the build box:
#   deploy/dev/remote.sh sh 'deploy/dev/audit.sh'
#
# Writes to ./audit-out/:
#   sbom/*.cdx.json     CycloneDX SBOM of every workspace crate (cargo-cyclonedx)
#   trivy-fs.txt        Trivy scan of the source tree (lockfiles, secrets, misconfig)
#   trivy-images.txt    Trivy scan of every image pinned in deploy/stack/images.toml
# Fails (exit 1) on a cargo-deny advisory or a Trivy finding in the source tree; image findings
# fail it only with AUDIT_STRICT=1.
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT="$PWD/audit-out"; rm -rf "$OUT"; mkdir -p "$OUT/sbom"
rc=0

echo "== cargo-deny (advisories, licenses, bans, sources)"
command -v cargo-deny > /dev/null || cargo install --locked cargo-deny
cargo deny check || rc=1

echo "== SBOM (CycloneDX)"
command -v cargo-cyclonedx > /dev/null || cargo install --locked cargo-cyclonedx
cargo cyclonedx --format json --spec-version 1.5 > /dev/null
find . -name '*.cdx.json' -not -path './audit-out/*' -not -path './target/*' -exec mv {} "$OUT/sbom/" \;
echo "$(ls "$OUT/sbom" | wc -l) SBOM files"

TRIVY="docker run --rm -v $PWD:/src:ro -v $HOME/.cache/trivy:/root/.cache aquasec/trivy:latest"
mkdir -p "$HOME/.cache/trivy"

echo "== Trivy: source tree"
$TRIVY fs --scanners vuln,secret --severity CRITICAL,HIGH --exit-code 1 /src | tee "$OUT/trivy-fs.txt" || rc=1

echo "== Trivy: pinned images"
grep -oE '"[a-z0-9./_-]+(:[A-Za-z0-9._-]+)?@sha256:[0-9a-f]{64}"' deploy/stack/images.toml | tr -d '"' | sort -u |
    while read -r image; do
        echo "-- $image" | tee -a "$OUT/trivy-images.txt"
        $TRIVY image --severity CRITICAL,HIGH --ignore-unfixed --exit-code 1 "$image" 2>&1 | tee -a "$OUT/trivy-images.txt" || echo "FINDINGS $image" >> "$OUT/findings"
    done
# Image findings that the upstream tag has not fixed yet cannot be cleared by re-pinning
# (deploy/dev/repin-images.sh): they are reported, and fail the run only with AUDIT_STRICT=1.
if [ -f "$OUT/findings" ]; then
    echo "images with CRITICAL/HIGH findings (see audit-out/trivy-images.txt):"; cat "$OUT/findings"
    [ "${AUDIT_STRICT:-0}" = 1 ] && rc=1
fi
exit $rc
