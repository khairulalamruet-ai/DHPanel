#!/usr/bin/env bash
# Installs the DHPanel control plane, and by default a local node agent, on ONE new server
# (Ubuntu 24.04 or Debian 12, run as root). T19.01.
#
#   sudo ./install.sh --bin-dir ./bin --hostname panel.example.com --admin-email you@example.com
#
#   --bin-dir DIR        the release's binaries: dhpanel-control-api, dhctl, and (for the local
#                        node) dhpanel-agent and dhpanel-priv-helper
#   --hostname NAME      the DNS name the panel is served under (it must already point here;
#                        Caddy gets its certificate from it)
#   --admin-email MAIL   the first administrator's email
#   --admin-username U   the first administrator's login (default: admin)
#   --no-agent           install the control plane only (nodes are added from other servers)
#   --check              only look: what is there, what would be done; changes nothing
#
# Safe to run again: every step looks at what is already there, secrets and the CA are never
# replaced, an administrator is created only when there is none. When a step fails it says
# which and why; running again continues from there.
#
# What it installs: PostgreSQL 18 (PGDG), Redis, Caddy, ufw (22, 80, 443 and the agent port
# 8444), the control plane as the unprivileged user `dhpanel` under its hardened unit, the CA
# of the node channel, and with the local node: rootless Docker, the agent and the root helper.
set -euo pipefail

BIN_DIR=""; HOSTNAME_FQDN=""; ADMIN_EMAIL=""; ADMIN_USER=admin; WITH_AGENT=1; CHECK=0
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ETC=/etc/dhpanel; SECRETS=$ETC/secrets; STATE=/var/lib/dhpanel; AGENT_PORT=8444

while [ $# -gt 0 ]; do
    case "$1" in
        --bin-dir) BIN_DIR="${2:?--bin-dir needs a folder}"; shift 2 ;;
        --hostname) HOSTNAME_FQDN="${2:?--hostname needs a name}"; shift 2 ;;
        --admin-email) ADMIN_EMAIL="${2:?--admin-email needs an address}"; shift 2 ;;
        --admin-username) ADMIN_USER="${2:?--admin-username needs a name}"; shift 2 ;;
        --no-agent) WITH_AGENT=0; shift ;;
        --check) CHECK=1; shift ;;
        -h | --help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "install.sh: unknown option: $1 (see --help)" >&2; exit 64 ;;
    esac
done

step=""
say() { step="$*"; echo "==> $*"; }
die() { echo "install.sh: $*" >&2; exit 1; }
trap 'rc=$?; [ $rc -ne 0 ] && [ -n "$step" ] && echo "install.sh: stopped during \"$step\"; fix the cause above and run this again" >&2' EXIT

# --- what this machine is ------------------------------------------------------------------
[ "$(uname -s)" = Linux ] || die "Linux only"
. /etc/os-release
case "${ID:-}:${VERSION_ID:-}" in
    ubuntu:24.04 | debian:12) CODENAME="$VERSION_CODENAME" ;;
    *) die "this installer supports Ubuntu 24.04 and Debian 12 (this is ${PRETTY_NAME:-unknown})" ;;
esac
[ "$(uname -m)" = x86_64 ] || [ "$(uname -m)" = aarch64 ] || die "x86_64 or aarch64 only"
command -v systemctl > /dev/null || die "systemd is required"
[ -n "$BIN_DIR" ] || die "give the release's binaries with --bin-dir DIR"
[ -n "$HOSTNAME_FQDN" ] || die "give the panel's DNS name with --hostname NAME"
[[ "$HOSTNAME_FQDN" =~ ^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$ ]] || die "--hostname must be a plain DNS name (lowercase letters, digits, dots, dashes)"
[ -n "$ADMIN_EMAIL" ] || die "give the first administrator's email with --admin-email"
[[ "$ADMIN_EMAIL" =~ ^[^@\ ]+@[^@\ ]+\.[^@\ ]+$ ]] || die "--admin-email does not look like an email address"
need=(dhpanel-control-api dhctl); [ "$WITH_AGENT" -eq 1 ] && need+=(dhpanel-agent dhpanel-priv-helper)
for b in "${need[@]}"; do
    [ -x "$BIN_DIR/$b" ] || die "$BIN_DIR/$b is missing or not executable"
    # An ELF program for this machine's architecture (the control plane has no --version: running
    # it would start the server).
    [ "$(head -c 4 "$BIN_DIR/$b" | od -An -c | tr -d ' ')" = '177ELF' ] || die "$BIN_DIR/$b is not a Linux program"
    machine=$(od -An -tx1 -j18 -N2 "$BIN_DIR/$b" | tr -d ' ')
    case "$(uname -m):$machine" in x86_64:3e00 | aarch64:b700) ;; *) die "$BIN_DIR/$b was built for another architecture ($machine)" ;; esac
done
free_gb=$(df -BG --output=avail / | tail -1 | tr -dc '0-9'); [ "$free_gb" -ge 10 ] || die "at least 10 GB free on / are needed (there are ${free_gb})"
mem_mb=$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo); [ "$mem_mb" -ge 1800 ] || die "at least 2 GB of memory are needed (there are ${mem_mb} MB)"

if [ "$CHECK" -eq 1 ]; then
    say "check only: nothing is changed"
    echo "    system:        ${PRETTY_NAME} ($(uname -m)), ${mem_mb} MB memory, ${free_gb} GB free"
    echo "    binaries:      ${need[*]} found and runnable"
    echo "    postgresql 18: $(command -v /usr/lib/postgresql/18/bin/postgres > /dev/null && echo present || echo 'would be installed')"
    echo "    redis:         $(command -v redis-server || echo 'would be installed')"
    echo "    caddy:         $(command -v caddy || echo 'would be installed')"
    echo "    panel user:    $(id dhpanel > /dev/null 2>&1 && echo present || echo 'would be created')"
    echo "    configuration: $([ -f $ETC/control.toml ] && echo "present ($ETC/control.toml is kept)" || echo 'would be written')"
    echo "    CA:            $([ -f $SECRETS/node_ca.crt ] && echo 'present (kept)' || echo 'would be created')"
    exit 0
fi
[ "$(id -u)" -eq 0 ] || die "run as root (sudo)"

export DEBIAN_FRONTEND=noninteractive
apt_install() { apt-get install -y -qq "$@" > /dev/null; }

# --- packages --------------------------------------------------------------------------------
say "base packages"
apt-get update -qq > /dev/null
apt_install ca-certificates curl gnupg openssl ufw uidmap dbus-user-session

say "PostgreSQL 18"
if [ ! -x /usr/lib/postgresql/18/bin/postgres ]; then
    install -d -m 0755 /usr/share/keyrings
    curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor --yes -o /usr/share/keyrings/postgresql.gpg
    echo "deb [signed-by=/usr/share/keyrings/postgresql.gpg] https://apt.postgresql.org/pub/repos/apt ${CODENAME}-pgdg main" > /etc/apt/sources.list.d/pgdg.list
    apt-get update -qq > /dev/null
    apt_install postgresql-18
fi
systemctl enable --now postgresql > /dev/null 2>&1
for _ in $(seq 1 30); do runuser -u postgres -- psql -qAt -c 'select 1' > /dev/null 2>&1 && break; sleep 1; done
runuser -u postgres -- psql -qAt -c 'select 1' > /dev/null 2>&1 || die "PostgreSQL does not answer (journalctl -u postgresql)"

say "Redis"
command -v redis-server > /dev/null || apt_install redis-server
systemctl enable --now redis-server > /dev/null 2>&1

say "Caddy"
if ! command -v caddy > /dev/null; then
    curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key | gpg --dearmor --yes -o /usr/share/keyrings/caddy.gpg
    echo "deb [signed-by=/usr/share/keyrings/caddy.gpg] https://dl.cloudsmith.io/public/caddy/stable/deb/debian any-version main" > /etc/apt/sources.list.d/caddy.list
    apt-get update -qq > /dev/null
    apt_install caddy
fi

# --- user, folders, secrets ----------------------------------------------------------------
say "the panel's user and folders"
id dhpanel > /dev/null 2>&1 || useradd --system --user-group --home-dir "$STATE" --no-create-home --shell /usr/sbin/nologin dhpanel
install -d -m 0755 -o root -g dhpanel "$ETC"
install -d -m 0750 -o root -g dhpanel "$SECRETS"
install -d -m 0700 -o dhpanel -g dhpanel "$STATE"
for b in dhpanel-control-api dhctl; do install -m 0755 -o root -g root "$BIN_DIR/$b" /usr/local/bin/$b; done

secret() { # name, command that prints it: written once, never replaced
    if [ ! -s "$SECRETS/$1" ]; then
        (umask 077; "${@:2}" > "$SECRETS/$1")
    fi
    chown root:dhpanel "$SECRETS/$1"; chmod 0640 "$SECRETS/$1"
}
say "database and Redis passwords"
secret db_password bash -c 'openssl rand -hex 24'
secret redis_password bash -c 'openssl rand -hex 24'
secret master_key bash -c 'openssl rand -base64 32'
DBPW=$(cat $SECRETS/db_password); RPW=$(cat $SECRETS/redis_password)

say "the panel's database"
runuser -u postgres -- psql -qAt -v ON_ERROR_STOP=1 -v pw="$DBPW" <<'SQL' > /dev/null
SELECT 'CREATE ROLE dhpanel LOGIN' WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'dhpanel') \gexec
ALTER ROLE dhpanel PASSWORD :'pw';
SELECT 'CREATE DATABASE dhpanel OWNER dhpanel' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'dhpanel') \gexec
SQL
secret database_url printf 'postgres://dhpanel:%s@127.0.0.1/dhpanel' "$DBPW"

say "Redis password (local only)"
if ! grep -q "^requirepass" /etc/redis/redis.conf; then
    printf '\nbind 127.0.0.1 -::1\nrequirepass %s\n' "$RPW" >> /etc/redis/redis.conf
    systemctl restart redis-server
fi
secret redis_url printf 'redis://:%s@127.0.0.1:6379/' "$RPW"

# --- configuration and the CA ---------------------------------------------------------------
say "configuration"
if [ ! -f "$ETC/control.toml" ]; then
    cat > "$ETC/control.toml" <<TOML
listen = "127.0.0.1:8080"
trusted_proxies = ["127.0.0.1", "::1"]

[log]
format = "json"
filter = "info"

[database]
url_file = "$SECRETS/database_url"
max_connections = 20

[redis]
url_file = "$SECRETS/redis_url"

[secrets]
master_key_file = "$SECRETS/master_key"

[pki]
ca_dir = "$SECRETS"

[agent]
enrollment_token_minutes = 15
public_url = "https://$HOSTNAME_FQDN"
stream_listen = "0.0.0.0:$AGENT_PORT"
stream_url = "https://$HOSTNAME_FQDN:$AGENT_PORT"
TOML
    chown root:dhpanel "$ETC/control.toml"; chmod 0640 "$ETC/control.toml"
fi
export DHPANEL_CONFIG="$ETC/control.toml"
as_panel() { runuser -u dhpanel -- env DHPANEL_CONFIG="$DHPANEL_CONFIG" "$@"; }

say "the node channel's certificate authority"
[ -f "$SECRETS/node_ca.crt" ] || as_panel dhctl ca init --dir "$SECRETS" > /dev/null
[ -f "$SECRETS/server.crt" ] || as_panel dhctl ca issue-server --dir "$SECRETS" --dns "$HOSTNAME_FQDN" --out-dir "$SECRETS" > /dev/null
chmod 0750 "$SECRETS"; chmod 0640 "$SECRETS"/*.crt; chmod 0600 "$SECRETS"/*.key; chown dhpanel:dhpanel "$SECRETS"/*.key

say "database schema"
as_panel dhctl migrate > /dev/null

say "the first administrator"
ADMIN_PASSWORD=""
if [ "$(runuser -u postgres -- psql -qAt -d dhpanel -c "select count(*) from accounts where kind = 'admin'")" = 0 ]; then
    ADMIN_PASSWORD="$(openssl rand -base64 24 | tr -d '/+=' | cut -c1-24)-Aa1"
    printf '%s\n' "$ADMIN_PASSWORD" | as_panel dhctl admin create --username "$ADMIN_USER" --email "$ADMIN_EMAIL" --password-stdin > /dev/null
fi

# --- the service, the web server, the firewall ----------------------------------------------
say "the control plane's service"
install -m 0644 "$HERE/systemd/dhpanel-control-api.service" /etc/systemd/system/dhpanel-control-api.service
systemctl daemon-reload
systemctl enable dhpanel-control-api > /dev/null 2>&1
systemctl restart dhpanel-control-api
for _ in $(seq 1 40); do curl -fsS http://127.0.0.1:8080/api/v1/public/ready > /dev/null 2>&1 && break; sleep 0.5; done
curl -fsS http://127.0.0.1:8080/api/v1/public/ready > /dev/null 2>&1 || die "the control plane does not answer (journalctl -u dhpanel-control-api)"

say "Caddy in front of the panel"
cat > /etc/caddy/Caddyfile <<CADDY
$HOSTNAME_FQDN {
    encode zstd gzip
    reverse_proxy 127.0.0.1:8080
}
CADDY
systemctl enable caddy > /dev/null 2>&1
systemctl reload caddy 2> /dev/null || systemctl restart caddy

say "firewall (ufw: 22, 80, 443, $AGENT_PORT)"
ufw allow 22/tcp > /dev/null; ufw allow 80/tcp > /dev/null; ufw allow 443/tcp > /dev/null; ufw allow "$AGENT_PORT"/tcp > /dev/null
ufw --force enable > /dev/null

if [ "$WITH_AGENT" -eq 1 ]; then
    say "the local node (rootless Docker, the agent, the root helper)"
    if ! command -v dockerd-rootless-setuptool.sh > /dev/null; then
        install -m 0755 -d /etc/apt/keyrings
        curl -fsSL "https://download.docker.com/linux/${ID}/gpg" | gpg --dearmor --yes -o /etc/apt/keyrings/docker.gpg
        echo "deb [signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/${ID} ${CODENAME} stable" > /etc/apt/sources.list.d/docker.list
        apt-get update -qq > /dev/null
        apt_install docker-ce docker-ce-cli containerd.io docker-ce-rootless-extras
        # Only the agent's own rootless daemon is used: the system-wide one stays off.
        systemctl disable --now docker.service docker.socket > /dev/null 2>&1 || true
    fi
    "$HERE/agent/install.sh" --binary "$BIN_DIR/dhpanel-agent" --priv-helper "$BIN_DIR/dhpanel-priv-helper" --rootless-docker
fi

trap - EXIT
cat <<EOF

DHPanel is installed.

  Panel:     https://$HOSTNAME_FQDN   (the certificate arrives within a minute, once the name points here)
  Login:     $ADMIN_USER
$([ -n "$ADMIN_PASSWORD" ] && printf '  Password:  %s\n  (shown once, never stored: sign in and change it, then set up two-factor)\n' "$ADMIN_PASSWORD" || echo '  (the administrator already existed; its password is unchanged)')

Keep a copy of $SECRETS/master_key somewhere else: without it a lost server cannot be restored
(deploy/runbooks/system-restore.md).
$([ "$WITH_AGENT" -eq 1 ] && printf '\nThe local node is installed but not enrolled: add a node in the panel, then run the enrol command it\nshows as the agent user, then `systemctl enable --now dhpanel-agent` (see deploy/agent/install.sh).\n')
EOF
