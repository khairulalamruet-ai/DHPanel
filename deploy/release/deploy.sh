#!/usr/bin/env bash
# Upgrades a running panel server to a release (T19.11), run as root on that server from the
# unpacked release:
#
#   sudo ./release/deploy.sh [--bin-dir ./bin]
#
# Order: keep the old binaries, install the new ones, apply migrations, restart, wait until the
# panel is ready. When it does not become ready the old binaries are put back and restarted
# (migrations only add; the previous version runs on the migrated database - a release that
# breaks that rule must say so in its changelog and is not deployed with this script).
set -euo pipefail
BIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/bin"
[ "${1:-}" = --bin-dir ] && BIN_DIR="${2:?--bin-dir needs a folder}"
[ "$(id -u)" -eq 0 ] || { echo "deploy.sh: run as root" >&2; exit 1; }
for b in dhpanel-control-api dhctl; do [ -x "$BIN_DIR/$b" ] || { echo "deploy.sh: $BIN_DIR/$b is missing" >&2; exit 1; }; done
KEEP="/var/lib/dhpanel/previous-release"
install -d -m 0700 "$KEEP"
for b in dhpanel-control-api dhctl; do [ -f /usr/local/bin/$b ] && cp -p /usr/local/bin/$b "$KEEP/$b"; done
ready() { for _ in $(seq 1 60); do curl -fsS http://127.0.0.1:8080/api/v1/public/ready > /dev/null 2>&1 && return 0; sleep 1; done; return 1; }

echo "==> installing the new binaries"
for b in dhpanel-control-api dhctl; do install -m 0755 -o root -g root "$BIN_DIR/$b" "/usr/local/bin/$b.new" && mv -f "/usr/local/bin/$b.new" "/usr/local/bin/$b"; done
echo "==> migrating the database"
runuser -u dhpanel -- env DHPANEL_CONFIG=/etc/dhpanel/control.toml /usr/local/bin/dhctl migrate
echo "==> restarting the control plane"
systemctl restart dhpanel-control-api
if ready; then echo "==> the new version is up"; exit 0; fi
echo "deploy.sh: the new version did not become ready; putting the old one back" >&2
for b in dhpanel-control-api dhctl; do [ -f "$KEEP/$b" ] && install -m 0755 "$KEEP/$b" "/usr/local/bin/$b"; done
systemctl restart dhpanel-control-api
ready && echo "deploy.sh: the old version is running again" >&2 || echo "deploy.sh: the old version does not answer either: journalctl -u dhpanel-control-api" >&2
exit 1
