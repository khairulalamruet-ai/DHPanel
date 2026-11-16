#!/usr/bin/env bash
# Moves every entry of deploy/stack/images.toml to the digest its tag points at today (T19.08).
# Run on the build box: deploy/dev/remote.sh sh 'deploy/dev/repin-images.sh'
# Review the diff, run audit.sh, and commit; nothing here is applied to a running stack.
set -euo pipefail
cd "$(dirname "$0")/../.."
f=deploy/stack/images.toml
python3 - "$f" <<'PY'
import re, subprocess, sys
path = sys.argv[1]
text = open(path).read()
def repl(m):
    name, old, tag = m.group(2), m.group(3), m.group(5)
    ref = f"{name}:{tag}"
    subprocess.run(["docker", "pull", "-q", ref], check=True, stdout=subprocess.DEVNULL)
    out = subprocess.run(["docker", "image", "inspect", "--format", "{{range .RepoDigests}}{{println .}}{{end}}", ref],
                         check=True, capture_output=True, text=True).stdout.split()
    new = next(d for d in out if d.startswith(name + "@"))
    print(f"{ref}: {old[:19]} -> {new.split('@')[1][:19]}", file=sys.stderr)
    return m.group(1) + new + m.group(4) + m.group(5) + '"'
text = re.sub(r'(reference = ")([a-z0-9./_-]+)@(sha256:[0-9a-f]{64})("(?:(?!reference)[^\n]*\n)*?tag\s*= ")([^"]+)"', repl, text)
open(path, "w").write(text)
PY
