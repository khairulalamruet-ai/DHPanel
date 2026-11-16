#!/usr/bin/env bash
# Smoke test of a running panel (T19.11): the checks a release must pass on staging before it
# goes to production. Read-only: it signs nobody in and changes nothing.
#
#   deploy/release/smoke.sh https://staging.panel.example.com
#
# Exit status 0 when every check passed, 1 otherwise.
set -uo pipefail
BASE="${1:?usage: smoke.sh BASE_URL}"
BASE="${BASE%/}"
fail=0
check() { # name, condition result
    if [ "$2" = ok ]; then echo "ok    $1"; else echo "FAIL  $1"; fail=1; fi
}
get() { curl -sS --max-time 15 -o "${2:-/dev/null}" -w '%{http_code}' "$BASE$1" 2> /dev/null || echo 000; }
tmp="$(mktemp)"; trap 'rm -f "$tmp"' EXIT

[ "$(get /api/v1/public/health)" = 200 ] && r=ok || r=bad; check "liveness answers 200" $r
code=$(get /api/v1/public/ready "$tmp")
{ [ "$code" = 200 ] && ! grep -q '"status":"unavailable"' "$tmp"; } && r=ok || r=bad; check "readiness (database, Redis) answers 200" $r
[ "$(get /api/v1/auth/me)" = 401 ] && r=ok || r=bad; check "no session means 401 on /auth/me" $r
[ "$(get /api/v1/admin/administrators)" = 401 ] && r=ok || r=bad; check "no session means 401 on an admin route" $r
[ "$(get /api/v1/client/domains)" = 401 ] && r=ok || r=bad; check "no session means 401 on a client route" $r
[ "$(get /api/v1/no-such-route)" = 404 ] && r=ok || r=bad; check "an unknown route is 404, not a crash" $r
curl -sS --max-time 15 -D "$tmp" -o /dev/null "$BASE/api/v1/public/health" 2> /dev/null
for header in x-content-type-options x-frame-options referrer-policy cache-control; do
    grep -qi "^$header:" "$tmp" && r=ok || r=bad; check "security header $header" $r
done
code=$(curl -sS --max-time 15 -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{}' "$BASE/api/v1/client/domains" 2> /dev/null)
[ "$code" = 401 ] || [ "$code" = 403 ] && r=ok || r=bad; check "a write without a session is refused ($code)" $r
case "$BASE" in https://*) [[ "$(curl -sS --max-time 15 -o /dev/null -w '%{http_code}' "http://${BASE#https://}/" 2> /dev/null)" =~ ^30[1278]$ ]] && r=ok || r=bad; check "plain HTTP redirects to HTTPS" $r ;; esac
[ "$fail" = 0 ] && echo "smoke test passed" || echo "SMOKE TEST FAILED"
exit "$fail"
