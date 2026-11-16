#!/usr/bin/env bash
# Copies a release tarball to the server named in DEPLOY_HOST and runs its deploy script there
# (T19.11). The release workflow calls this; DEPLOY_SSH_KEY and DEPLOY_KNOWN_HOSTS hold the key
# and the host's key line (never ask the first time: a changed host key must fail the deploy).
#
#   DEPLOY_HOST=deploy@panel DEPLOY_SSH_KEY=... DEPLOY_KNOWN_HOSTS=... remote-deploy.sh dhpanel-1.0.0-linux-x86_64.tar.gz
set -euo pipefail
tarball="${1:?usage: remote-deploy.sh TARBALL}"
: "${DEPLOY_HOST:?}" "${DEPLOY_SSH_KEY:?}" "${DEPLOY_KNOWN_HOSTS:?}"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
(umask 077; printf '%s\n' "$DEPLOY_SSH_KEY" > "$work/key"; printf '%s\n' "$DEPLOY_KNOWN_HOSTS" > "$work/known_hosts")
ssh_opts=(-i "$work/key" -o IdentitiesOnly=yes -o BatchMode=yes -o StrictHostKeyChecking=yes -o UserKnownHostsFile="$work/known_hosts")
name="$(basename "$tarball" .tar.gz)"
scp "${ssh_opts[@]}" "$tarball" "$DEPLOY_HOST:/tmp/$name.tar.gz"
ssh "${ssh_opts[@]}" "$DEPLOY_HOST" "set -e; cd /tmp && rm -rf $name && tar -xzf $name.tar.gz && sudo -n $name/release/deploy.sh --bin-dir /tmp/$name/bin; rc=\$?; rm -rf /tmp/$name /tmp/$name.tar.gz; exit \$rc"
