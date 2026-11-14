#!/usr/bin/env bash
# provision_tenant_stack for real: helper as root, agent code as the dev user with its rootless Docker.
set -uo pipefail
BIN=/home/dev/target-rust/debug
ROOTDIR=/srv/tenants-live; SOCK=/tmp/dh-live-helper.sock
runuser -u dev -- bash -lc 'cd ~/work/DHPanel-claude && . ~/.cargo/env && CARGO_TARGET_DIR=~/target-rust SQLX_OFFLINE=true cargo test -q -p dhpanel-node-agent --test provision_live --no-run 2>&1 | tail -3; CARGO_TARGET_DIR=~/target-rust SQLX_OFFLINE=true cargo build -q -p dhpanel-priv-helper 2>&1 | tail -3'
TEST=$(ls -t $BIN/deps/provision_live-* | grep -v '\.d$' | head -1)
echo "== helper as root (agent user: dev, tenants in $ROOTDIR)"
rm -f $SOCK
$BIN/dhpanel-priv-helper --socket $SOCK --agent-user dev --tenants-root $ROOTDIR > /tmp/dh-live-helper.log 2>&1 &
HP=$!
for _ in $(seq 1 30); do [ -S $SOCK ] && break; sleep 0.2; done
echo "   socket: $(stat -c '%a %U' $SOCK)  capabilities of the helper: $(grep CapEff /proc/$HP/status | awk '{print $2}')"
echo "== the agent provisioning a stack (as dev)"
runuser -u dev -- env DHPANEL_TEST_HELPER_SOCKET=$SOCK DHPANEL_TEST_TENANTS_ROOT=$ROOTDIR DHPANEL_TEST_AGENT_USER=dev $TEST --include-ignored --nocapture 2>&1 | grep -E "^test |test result|panicked|left|right|expect|skipped|error" -A6 | head -50
echo "== on disk"
ls -ln $ROOTDIR | sed 's/^/   /'
for d in $ROOTDIR/*/; do echo "   $(stat -c '%a uid=%u gid=%g' $d) $(basename $d)  www: $(stat -c '%a uid=%u' $d/www)"; done
echo "== what the helper logged"
sed 's/^/   /' /tmp/dh-live-helper.log | tail -8
kill $HP 2>/dev/null; sleep 0.5
echo "== tidy up"
find $ROOTDIR -mindepth 1 -delete 2>/dev/null; rmdir $ROOTDIR 2>/dev/null; rm -f $SOCK
echo "   removed: $([ -e $ROOTDIR ] && echo no || echo yes)"
