#!/usr/bin/env bash
# Installs the DHPanel node agent on a Linux server (run as root).
#
#   sudo ./install.sh --binary ./dhpanel-agent [--priv-helper PATH | --no-priv-helper] [--rootless-docker] [--check]
#
# The root helper is installed (or updated) automatically from the file called
# dhpanel-priv-helper next to the agent binary; --priv-helper PATH names another,
# --no-priv-helper leaves it out. Running this again after an upgrade replaces the
# binaries and the units and restarts the helper: nothing else to do by hand.
# --check only looks (what is there, what would be done) and changes nothing.
#
# What it does (safe to run again):
#   * creates the unprivileged system user `dhpanel-agent`
#   * installs the binary to /usr/local/bin and the hardened systemd unit
#   * creates /var/lib/dhpanel-agent (mode 0700, owned by the agent)
#   * installs and starts the small root helper the agent asks for the few host
#     changes that need root (its own hardened unit), and the packet filter tool
#     (nftables) it needs for the database's remote access, if it is missing
#   * with --rootless-docker: gives the agent its own rootless Docker daemon
#
# It does NOT enrol or start the agent: enrol first, with the command the panel
# shows when you add the node (run as the agent user), then enable the service.
set -euo pipefail

AGENT_USER=dhpanel-agent
STATE_DIR=/var/lib/dhpanel-agent
BIN_DST=/usr/local/bin/dhpanel-agent
UNIT_DST=/etc/systemd/system/dhpanel-agent.service
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

binary=""
helper=""
no_helper=0
check=0
rootless=0
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) binary="${2:?--binary needs a path}"; shift 2 ;;
    --priv-helper) helper="${2:?--priv-helper needs a path}"; shift 2 ;;
    --no-priv-helper) no_helper=1; shift ;;
    --check) check=1; shift ;;
    --rootless-docker) rootless=1; shift ;;
    -h|--help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "install.sh: unknown option: $1" >&2; exit 64 ;;
  esac
done

die() { echo "install.sh: $*" >&2; exit 1; }
say() { echo "==> $*"; }

if [ "$check" -eq 0 ]; then
  [ "$(id -u)" -eq 0 ] || die "run as root (sudo)"
fi
[ "$(uname -s)" = Linux ] || die "Linux only"
command -v systemctl >/dev/null || die "systemd is required"
[ -n "$binary" ] || die "give the agent binary with --binary PATH"
[ -x "$binary" ] || die "$binary is not an executable file"
"$binary" --version >/dev/null 2>&1 || die "$binary does not run on this machine"

# The helper: the one named, or the one beside the agent binary.
if [ "$no_helper" -eq 1 ]; then
  helper=""
elif [ -z "$helper" ] && [ -x "$(dirname "$binary")/dhpanel-priv-helper" ]; then
  helper="$(dirname "$binary")/dhpanel-priv-helper"
fi

if [ "$check" -eq 1 ]; then
  say "check only: nothing is changed"
  say "agent binary:   $binary ($("$binary" --version 2>/dev/null | head -1))"
  if [ -n "$helper" ]; then
    "$helper" --version >/dev/null 2>&1 || die "$helper does not run on this machine"
    say "root helper:    $helper ($("$helper" --version 2>/dev/null | head -1)) would be installed"
  else
    say "root helper:    none (use --priv-helper PATH, or put dhpanel-priv-helper next to the agent binary)"
  fi
  say "nftables:       $(command -v nft || echo 'missing (would be installed)')"
  say "agent user:     $(id "$AGENT_USER" >/dev/null 2>&1 && echo present || echo 'would be created')"
  exit 0
fi

# --- user and directories ---------------------------------------------------
if id "$AGENT_USER" >/dev/null 2>&1; then
  say "user $AGENT_USER exists"
else
  say "creating user $AGENT_USER"
  # The home directory is inside the state directory so the rootless Docker
  # data of the agent lives (and stays private) with it.
  useradd --system --user-group --home-dir "$STATE_DIR/home" --no-create-home \
          --shell /usr/sbin/nologin "$AGENT_USER"
fi
AGENT_UID="$(id -u "$AGENT_USER")"
install -d -m 0700 -o "$AGENT_USER" -g "$AGENT_USER" "$STATE_DIR" "$STATE_DIR/home"
# Files on their way between the network and a tenant's files: the agent and (through its
# group) the privileged helper only.
install -d -m 0770 -o "$AGENT_USER" -g "$AGENT_USER" /var/lib/dhpanel-transfers

# --- binary and unit --------------------------------------------------------
say "installing $BIN_DST"
install -m 0755 -o root -g root "$binary" "$BIN_DST"
say "installing $UNIT_DST"
sed "s/@AGENT_UID@/${AGENT_UID}/g" "$HERE/dhpanel-agent.service" > "$UNIT_DST.new"
chmod 0644 "$UNIT_DST.new"
mv -f "$UNIT_DST.new" "$UNIT_DST"
if command -v systemd-analyze >/dev/null; then
  systemd-analyze verify "$UNIT_DST" 2>&1 | grep -v "^$" | sed 's/^/    /' || true
fi
systemctl daemon-reload

# --- privileged helper -------------------------------------------------------
if [ -n "$helper" ]; then
  [ -x "$helper" ] || die "$helper is not an executable file"
  "$helper" --version >/dev/null 2>&1 || die "$helper does not run on this machine"
  # The packet filter tool the helper uses for the database's remote access.
  if ! command -v nft >/dev/null 2>&1 && [ ! -x /usr/sbin/nft ]; then
    say "installing nftables"
    if command -v apt-get >/dev/null; then
      DEBIAN_FRONTEND=noninteractive apt-get install -y nftables >/dev/null
    elif command -v dnf >/dev/null; then
      dnf install -y nftables >/dev/null
    elif command -v yum >/dev/null; then
      yum install -y nftables >/dev/null
    elif command -v zypper >/dev/null; then
      zypper --non-interactive install nftables >/dev/null
    else
      die "nftables is missing and no known package manager was found: install it, then run this again"
    fi
  fi
  [ -x /usr/sbin/nft ] || die "nftables is installed but /usr/sbin/nft is not there (the helper runs it by that path)"
  HELPER_DST=/usr/local/bin/dhpanel-priv-helper
  HELPER_UNIT=/etc/systemd/system/dhpanel-priv-helper.service
  say "installing $HELPER_DST"
  install -m 0755 -o root -g root "$helper" "$HELPER_DST.new"
  mv -f "$HELPER_DST.new" "$HELPER_DST"
  install -m 0644 -o root -g root "$HERE/dhpanel-priv-helper.service" "$HELPER_UNIT"
  systemctl daemon-reload
  # Where tenants live. It must exist before the unit starts (the unit's sandbox
  # lists it as writable); only root may create things in it.
  install -d -m 0755 -o root -g root /srv/tenants
  say "starting dhpanel-priv-helper"
  systemctl enable dhpanel-priv-helper >/dev/null 2>&1
  systemctl restart dhpanel-priv-helper
  for _ in $(seq 1 20); do
    [ -S /run/dhpanel/priv-helper.sock ] && break
    sleep 0.5
  done
  [ -S /run/dhpanel/priv-helper.sock ] || die "the privileged helper did not create its socket (journalctl -u dhpanel-priv-helper)"
else
  say "skipping the privileged helper (none given or found next to the agent binary)"
fi

# --- rootless Docker for the agent -------------------------------------------
if [ "$rootless" -eq 1 ]; then
  command -v dockerd-rootless-setuptool.sh >/dev/null \
    || die "dockerd-rootless-setuptool.sh not found: install docker-ce-rootless-extras (and uidmap) first"
  command -v newuidmap >/dev/null || die "newuidmap not found: install the uidmap package"
  command -v loginctl >/dev/null || die "loginctl not found"

  if ! grep -q "^${AGENT_USER}:" /etc/subuid 2>/dev/null; then
    # First free block of 65536 ids after everything already handed out.
    start=$(awk -F: 'BEGIN{m=100000} {e=$2+$3; if (e>m) m=e} END{print m}' /etc/subuid /etc/subgid 2>/dev/null)
    say "allocating subordinate ids ${start}-$((start + 65535)) for $AGENT_USER"
    usermod --add-subuids "${start}-$((start + 65535))" --add-subgids "${start}-$((start + 65535))" "$AGENT_USER"
  fi
  say "enabling lingering so the agent's Docker daemon runs without a login"
  loginctl enable-linger "$AGENT_USER"
  for _ in $(seq 1 30); do
    systemctl is-active --quiet "user@${AGENT_UID}.service" && break
    sleep 1
  done
  systemctl is-active --quiet "user@${AGENT_UID}.service" || die "user@${AGENT_UID}.service did not start"

  as_agent() {
    runuser -u "$AGENT_USER" -- env HOME="$STATE_DIR/home" \
      XDG_RUNTIME_DIR="/run/user/${AGENT_UID}" \
      DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${AGENT_UID}/bus" \
      PATH="/usr/bin:/usr/sbin:/bin:/sbin" "$@"
  }
  say "setting up rootless Docker for $AGENT_USER"
  as_agent dockerd-rootless-setuptool.sh install >/dev/null
  # The real address of a visitor must reach the containers (the SFTP server counts failed
  # logins per address, T11.07). Rootless Docker's default port driver (builtin) shows every
  # container the bridge gateway instead; slirp4netns passes the true source address on.
  dropin="$STATE_DIR/home/.config/systemd/user/docker.service.d"
  want=$'[Service]\nEnvironment=DOCKERD_ROOTLESS_ROOTLESSKIT_PORT_DRIVER=slirp4netns\n'
  if [ "$(cat "$dropin/port-driver.conf" 2>/dev/null)" != "${want%$'\n'}" ]; then
    say "using the slirp4netns port driver so containers see real client addresses"
    as_agent mkdir -p "$dropin"
    printf '%s' "$want" | as_agent tee "$dropin/port-driver.conf" >/dev/null
    as_agent systemctl --user daemon-reload >/dev/null 2>&1 || true
    # A running daemon picks the driver up only on a restart (its containers are recreated
    # by the agent's next convergence); a fresh install is started below.
    as_agent systemctl --user is-active --quiet docker && as_agent systemctl --user restart docker >/dev/null 2>&1 || true
  fi
  as_agent systemctl --user enable --now docker >/dev/null 2>&1 || true
  for _ in $(seq 1 30); do
    [ -S "/run/user/${AGENT_UID}/docker.sock" ] && break
    sleep 1
  done
  [ -S "/run/user/${AGENT_UID}/docker.sock" ] || die "the agent's Docker socket did not appear"
  say "rootless Docker is up: $(as_agent env DOCKER_HOST="unix:///run/user/${AGENT_UID}/docker.sock" docker version --format '{{.Server.Version}} (rootless={{range .Server.Components}}{{.Name}} {{end}})' 2>/dev/null | cut -c1-80)"
else
  say "skipping rootless Docker (use --rootless-docker to set it up for the agent)"
fi

cat <<EOF

Installed.$([ -n "$helper" ] && printf '\n\nThe database server is reached from outside the node on port 13306 only by the\naddresses a client lists, but if this machine sits behind a provider firewall, let\nthat port through there too.')

Next:
  1. Enrol (once, as the agent user), using the command shown when you add the node in the panel:
       sudo -u $AGENT_USER dhpanel-agent enroll --server … --ca-fingerprint … --token …
  2. Start the service:
       sudo systemctl enable --now dhpanel-agent
  3. Check it:
       sudo -u $AGENT_USER dhpanel-agent status
       systemctl status dhpanel-agent
EOF
