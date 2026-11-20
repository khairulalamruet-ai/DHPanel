#!/usr/bin/env bash
# Installs the Built Panel control plane, and by default a local node agent, on ONE new server
# (Ubuntu 22.04/24.04/26.04 or Debian 12/13, run as root). T19.01.
#
#   sudo ./install.sh --bin-dir ./bin --hostname panel.example.com --admin-email you@example.com
#
#   --bin-dir DIR        the release's binaries: builtpanel-control-api, bpctl, builtpanel-root-auth, and (for the local
#                        node) builtpanel-agent and builtpanel-priv-helper
#   --hostname NAME      the DNS name the panel is served under (it must already point here;
#                        Caddy gets its certificate from it)
#   --admin-email MAIL   the first administrator's email
#   --admin-username U   the first administrator's login (default: owner; "admin" and other system names are reserved)
#   --web-dir DIR        the built web interface: the two archives of a release (DIR/web-client and
#                        DIR/web-admin) or the two built folders (DIR/client and DIR/admin). Default:
#                        the web folder of the release this script is in, when there is one. Without
#                        a web interface only the API is served.
#   --admin-hostname N   the name the administrators' app is served under (default: admin.<hostname>;
#                        it must point here too; on sslip.io names it does on its own)
#   --no-agent           install the control plane only (nodes are added from other servers)
#   --no-root-login      do not let `root` log in to the panel with this server's root password
#                        (on by default, like WHM; the helper builtpanel-root-auth checks it)
#   --check              only look: what is there, what would be done; changes nothing
#
# Safe to run again: every step looks at what is already there, secrets and the CA are never
# replaced, an administrator is created only when there is none. When a step fails it says
# which and why; running again continues from there.
#
# What it installs: PostgreSQL 18 (PGDG), Redis, Caddy, ufw (22, 80, 443 and the agent port
# 8444), the control plane as the unprivileged user `builtpanel` under its hardened unit, the CA
# of the node channel, and with the local node: rootless Docker, the agent and the root helper.
set -euo pipefail

BIN_DIR=""; WEB_DIR=""; ADMIN_HOSTNAME=""; HOSTNAME_FQDN=""; ADMIN_EMAIL=""; ADMIN_USER=owner; WITH_AGENT=1; ROOT_LOGIN=1; CHECK=0
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ETC=/etc/builtpanel; SECRETS=$ETC/secrets; STATE=/var/lib/builtpanel; AGENT_PORT=8444

while [ $# -gt 0 ]; do
    case "$1" in
        --bin-dir) BIN_DIR="${2:?--bin-dir needs a folder}"; shift 2 ;;
        --hostname) HOSTNAME_FQDN="${2:?--hostname needs a name}"; shift 2 ;;
        --web-dir) WEB_DIR="${2:?--web-dir needs a folder}"; shift 2 ;;
        --admin-hostname) ADMIN_HOSTNAME="${2:?--admin-hostname needs a name}"; shift 2 ;;
        --admin-email) ADMIN_EMAIL="${2:?--admin-email needs an address}"; shift 2 ;;
        --admin-username) ADMIN_USER="${2:?--admin-username needs a name}"; shift 2 ;;
        --no-agent) WITH_AGENT=0; shift ;;
        --no-root-login) ROOT_LOGIN=0; shift ;;
        --check) CHECK=1; shift ;;
        -h | --help) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "install.sh: unknown option: $1 (see --help)" >&2; exit 64 ;;
    esac
done

# The look: the logo, a live line per step with an overall progress bar (deploy/ui.sh), or plain
# "==> step" lines when that file is missing or the output is not a terminal.
if [ -f "$HERE/ui.sh" ]; then
    # shellcheck source=deploy/ui.sh
    . "$HERE/ui.sh"
else
    ui_init() { exec 3>&1 4>&2; }
    ui_banner() { :; }
    ui_log_to() { :; }
    ui_step() { echo "==> $1"; }
    ui_warn() { echo "warning: $1"; }
    ui_fail() { echo "install.sh: $1" >&2; }
    ui_done() { :; }
    ui_box() { cat >&3; }
    ui_cleanup() { :; }
    ui_is_main() { :; }
fi
UI_TOTAL=$(grep -c '^[[:space:]]*say "' "${BASH_SOURCE[0]}")
ui_init
[ "${BUILTPANEL_UI_BANNER:-1}" = 0 ] || ui_banner "the modern hosting control panel  ·  installer"

step=""
say() { step="$*"; ui_step "$*"; }
die() { ui_fail "$*"; exit 1; }
trap 'rc=$?; ui_is_main || exit $rc; ui_cleanup; [ $rc -ne 0 ] && [ -n "$step" ] && echo "install.sh: stopped during \"$step\"; fix the cause above and run this again" >&4' EXIT

# --- what this machine is ------------------------------------------------------------------
[ "$(uname -s)" = Linux ] || die "Linux only"
. /etc/os-release
case "${ID:-}:${VERSION_ID:-}" in
    ubuntu:22.04 | ubuntu:24.04 | ubuntu:26.04 | debian:12 | debian:13) CODENAME="$VERSION_CODENAME" ;;
    ubuntu:20.04 | ubuntu:18.04 | ubuntu:16.04)
        die "Ubuntu ${VERSION_ID} is not supported: it starts with the old cgroup v1 (a hosting account's memory and CPU limits do not work with rootless Docker there), has kernel 5.4 and no PostgreSQL 18 packages. Use Ubuntu 22.04 or newer." ;;
    centos:7* | rhel:7* | ol:7* | rocky:7*)
        die "${PRETTY_NAME:-this system} is not supported: kernel 3.10 cannot run rootless containers, there is no PostgreSQL 18 for it, and it is out of support. Use Ubuntu 22.04 or newer, Debian 12 or newer." ;;
    almalinux:* | rocky:* | rhel:* | centos:* | ol:*)
        die "${PRETTY_NAME:-this system} (the Red Hat family) is not supported by this installer yet: it needs dnf, firewalld and SELinux handling. Use Ubuntu 22.04 or newer, Debian 12 or newer." ;;
    *) die "this installer supports Ubuntu 22.04, 24.04 and 26.04 and Debian 12 and 13 (this is ${PRETTY_NAME:-unknown})" ;;
esac
[ "$(uname -m)" = x86_64 ] || [ "$(uname -m)" = aarch64 ] || die "x86_64 or aarch64 only"
# Resource limits of rootless containers need cgroup v2 (the default on every system above).
[ "$(stat -fc %T /sys/fs/cgroup 2> /dev/null)" = cgroup2fs ] || die "cgroup v2 is not in use (the unified hierarchy): hosting accounts' memory and CPU limits would not work"
command -v systemctl > /dev/null || die "systemd is required"
[ -n "$BIN_DIR" ] || die "give the release's binaries with --bin-dir DIR"
[ -n "$HOSTNAME_FQDN" ] || die "give the panel's DNS name with --hostname NAME"
[[ "$HOSTNAME_FQDN" =~ ^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$ ]] || die "--hostname must be a plain DNS name (lowercase letters, digits, dots, dashes)"
[ -n "$WEB_DIR" ] || { [ -d "$HERE/web" ] && WEB_DIR="$HERE/web"; } || true
if [ -n "$WEB_DIR" ]; then
    if [ -f "$WEB_DIR/web-client" ] && [ -f "$WEB_DIR/web-admin" ]; then WEB_FORM=archives
    elif [ -f "$WEB_DIR/client/index.html" ] && [ -f "$WEB_DIR/admin/index.html" ]; then WEB_FORM=folders
    else die "$WEB_DIR must hold web-client and web-admin (a release's archives) or client/index.html and admin/index.html (the built folders)"; fi
    [ -n "$ADMIN_HOSTNAME" ] || ADMIN_HOSTNAME="admin.$HOSTNAME_FQDN"
    [[ "$ADMIN_HOSTNAME" =~ ^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$ ]] || die "--admin-hostname must be a plain DNS name"
fi
[ -n "$ADMIN_EMAIL" ] || die "give the first administrator's email with --admin-email"
[[ "$ADMIN_EMAIL" =~ ^[^@\ ]+@[^@\ ]+\.[^@\ ]+$ ]] || die "--admin-email does not look like an email address"
need=(builtpanel-control-api bpctl); [ "$WITH_AGENT" -eq 1 ] && need+=(builtpanel-agent builtpanel-priv-helper); [ "$ROOT_LOGIN" -eq 1 ] && need+=(builtpanel-root-auth)
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
    echo "==> check only: nothing is changed"
    echo "    system:        ${PRETTY_NAME} ($(uname -m)), ${mem_mb} MB memory, ${free_gb} GB free"
    echo "    binaries:      ${need[*]} found and runnable"
    echo "    postgresql 18: $(command -v /usr/lib/postgresql/18/bin/postgres > /dev/null && echo present || echo 'would be installed')"
    echo "    redis:         $(command -v redis-server || echo 'would be installed')"
    echo "    caddy:         $(command -v caddy || echo 'would be installed')"
    echo "    web interface: $([ -n "$WEB_DIR" ] && echo "$WEB_DIR (clients on $HOSTNAME_FQDN, administrators on $ADMIN_HOSTNAME)" || echo 'none given: only the API is served')"
    echo "    panel user:    $(id builtpanel > /dev/null 2>&1 && echo present || echo 'would be created')"
    echo "    configuration: $([ -f $ETC/control.toml ] && echo "present ($ETC/control.toml is kept)" || echo 'would be written')"
    echo "    CA:            $([ -f $SECRETS/node_ca.crt ] && echo 'present (kept)' || echo 'would be created')"
    echo "    root login:    $([ "$ROOT_LOGIN" -eq 1 ] && echo "on (root's password: $(passwd -S root 2> /dev/null | awk '{print $2}'))" || echo off)"
    exit 0
fi
[ "$(id -u)" -eq 0 ] || die "run as root (sudo)"
# From here the commands' own output goes to the log; the terminal shows the steps.
ui_log_to /var/log/builtpanel-install.log

export DEBIAN_FRONTEND=noninteractive
apt_install() { apt-get install -y -qq "$@" > /dev/null; }

# --- packages --------------------------------------------------------------------------------
say "base packages"
apt-get update -qq > /dev/null
# slirp4netns: rootless Docker's network driver (newer Docker releases no longer bring it along).
apt_install ca-certificates curl gnupg openssl ufw uidmap dbus-user-session slirp4netns

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
if ! command -v redis-server > /dev/null; then
    # The panel needs Redis 6.2 or newer (GETDEL). Ubuntu 22.04 ships 6.0, so there Redis is taken
    # from the Redis project's own repository; the other systems' own packages are new enough.
    if [ "${ID}:${VERSION_ID}" = ubuntu:22.04 ]; then
        curl -fsSL https://packages.redis.io/gpg | gpg --dearmor --yes -o /usr/share/keyrings/redis-archive-keyring.gpg
        echo "deb [signed-by=/usr/share/keyrings/redis-archive-keyring.gpg] https://packages.redis.io/deb ${CODENAME} main" > /etc/apt/sources.list.d/redis.list
        apt-get update -qq > /dev/null
    fi
    apt_install redis-server
fi
redis_version="$(redis-server --version | sed -n 's/.*v=\([0-9.]*\).*/\1/p')"
[ "$(printf '%s\n6.2\n' "$redis_version" | sort -V | head -1)" = 6.2 ] || die "Redis $redis_version is too old: the panel needs 6.2 or newer"
systemctl enable --now redis-server > /dev/null 2>&1

say "Caddy"
if ! command -v caddy > /dev/null; then
    curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key | gpg --dearmor --yes -o /usr/share/keyrings/caddy.gpg
    echo "deb [signed-by=/usr/share/keyrings/caddy.gpg] https://dl.cloudsmith.io/public/caddy/stable/deb/debian any-version main" > /etc/apt/sources.list.d/caddy.list
    apt-get update -qq > /dev/null
    apt_install caddy
fi

# --- user, folders, secrets ----------------------------------------------------------------
# --- upgrade from DHPanel -------------------------------------------------------------------
# A server installed while the panel was called DHPanel moves to the new names: services, the
# system user, folders, the database and its role. Secrets, the CA and the data stay as they are.
if [ -d /etc/dhpanel ] && [ ! -e "$ETC" ]; then
    say "upgrading from DHPanel to Built Panel"
    for unit in dhpanel-control-api dhpanel-root-auth; do
        systemctl disable --now "$unit" > /dev/null 2>&1 || true
        rm -f "/etc/systemd/system/$unit.service"
    done
    # The node agent's part (its user, Docker and packet filter) is moved by agent/install.sh.
    systemctl stop dhpanel-agent 2> /dev/null || true
    systemctl daemon-reload
    if id dhpanel > /dev/null 2>&1 && ! id builtpanel > /dev/null 2>&1; then
        usermod -l builtpanel -d "$STATE" dhpanel
        groupmod -n builtpanel dhpanel 2> /dev/null || true
    fi
    mv /etc/dhpanel "$ETC"
    [ ! -d /var/lib/dhpanel ] || [ -e "$STATE" ] || mv /var/lib/dhpanel "$STATE"
    [ ! -d /usr/share/dhpanel ] || [ -e /usr/share/builtpanel ] || mv /usr/share/dhpanel /usr/share/builtpanel
    sed -i 's#/etc/dhpanel#/etc/builtpanel#g; s#/var/lib/dhpanel#/var/lib/builtpanel#g; s#/run/dhpanel#/run/builtpanel#g; s#/usr/share/dhpanel#/usr/share/builtpanel#g' "$ETC/control.toml"
    runuser -u postgres -- psql -qAt -v ON_ERROR_STOP=1 <<'SQL' > /dev/null
SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'dhpanel' AND pid <> pg_backend_pid();
SELECT 'ALTER DATABASE dhpanel RENAME TO builtpanel' WHERE EXISTS (SELECT FROM pg_database WHERE datname = 'dhpanel') AND NOT EXISTS (SELECT FROM pg_database WHERE datname = 'builtpanel') \gexec
SELECT 'ALTER ROLE dhpanel RENAME TO builtpanel' WHERE EXISTS (SELECT FROM pg_roles WHERE rolname = 'dhpanel') AND NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'builtpanel') \gexec
SQL
    # The address names the role and the database (its password is set again below).
    [ ! -f "$SECRETS/database_url" ] || sed -i 's#//dhpanel:#//builtpanel:#; s#/dhpanel$#/builtpanel#' "$SECRETS/database_url"
    rm -f /usr/local/bin/dhpanel-control-api /usr/local/bin/dhctl /usr/local/bin/dhpanel-root-auth
fi

say "the panel's user and folders"
id builtpanel > /dev/null 2>&1 || useradd --system --user-group --home-dir "$STATE" --no-create-home --shell /usr/sbin/nologin builtpanel
install -d -m 0755 -o root -g builtpanel "$ETC"
install -d -m 0750 -o root -g builtpanel "$SECRETS"
install -d -m 0700 -o builtpanel -g builtpanel "$STATE"
for b in builtpanel-control-api bpctl; do install -m 0755 -o root -g root "$BIN_DIR/$b" /usr/local/bin/$b; done
[ "$ROOT_LOGIN" -eq 0 ] || install -m 0755 -o root -g root "$BIN_DIR/builtpanel-root-auth" /usr/local/bin/builtpanel-root-auth

secret() { # name, command that prints it: written once, never replaced
    if [ ! -s "$SECRETS/$1" ]; then
        (umask 077; "${@:2}" > "$SECRETS/$1")
    fi
    # The panel's own user reads it and nobody else: the program refuses a secret file that
    # anyone but its owner can read.
    chown builtpanel:builtpanel "$SECRETS/$1"; chmod 0600 "$SECRETS/$1"
}
say "database and Redis passwords"
secret db_password bash -c 'openssl rand -hex 24'
secret redis_password bash -c 'openssl rand -hex 24'
secret master_key bash -c 'openssl rand -base64 32'
DBPW=$(cat $SECRETS/db_password); RPW=$(cat $SECRETS/redis_password)

say "the panel's database"
runuser -u postgres -- psql -qAt -v ON_ERROR_STOP=1 -v pw="$DBPW" <<'SQL' > /dev/null
SELECT 'CREATE ROLE builtpanel LOGIN' WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'builtpanel') \gexec
ALTER ROLE builtpanel PASSWORD :'pw';
SELECT 'CREATE DATABASE builtpanel OWNER builtpanel' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'builtpanel') \gexec
SQL
secret database_url printf 'postgres://builtpanel:%s@127.0.0.1/builtpanel' "$DBPW"

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
    chown root:builtpanel "$ETC/control.toml"; chmod 0640 "$ETC/control.toml"
fi
# Root login (like WHM): the control plane asks the root helper on this socket. Kept in step with
# --no-root-login on every run, also in a configuration written by an earlier run.
ROOT_SOCKET=/run/builtpanel-root-auth/root-auth.sock
sed -i '/^root_login_socket *=/d' "$ETC/control.toml"
if [ "$ROOT_LOGIN" -eq 1 ]; then
    if grep -q '^\[auth\]' "$ETC/control.toml"; then
        sed -i "/^\[auth\]/a root_login_socket = \"$ROOT_SOCKET\"" "$ETC/control.toml"
    else
        printf '\n[auth]\nroot_login_socket = "%s"\n' "$ROOT_SOCKET" >> "$ETC/control.toml"
    fi
fi
export BUILTPANEL_CONFIG="$ETC/control.toml"
as_panel() { runuser -u builtpanel -- env BUILTPANEL_CONFIG="$BUILTPANEL_CONFIG" "$@"; }

say "the node channel's certificate authority"
# Made by root (the folder is not writable for the panel's user); the keys are handed over below.
[ -f "$SECRETS/node_ca.crt" ] || bpctl ca init --dir "$SECRETS" > /dev/null
[ -f "$SECRETS/server.crt" ] || bpctl ca issue-server --dir "$SECRETS" --dns "$HOSTNAME_FQDN" --out-dir "$SECRETS" > /dev/null
chmod 0750 "$SECRETS"; chmod 0640 "$SECRETS"/*.crt; chmod 0600 "$SECRETS"/*.key
# Made by root, read by the panel's user (the certificates are not secret, the keys are).
chown builtpanel:builtpanel "$SECRETS"/*.crt "$SECRETS"/*.key

say "database schema"
as_panel bpctl migrate > /dev/null

CPU_LIMITS=yes
ROOT_PW_STATE=""
if [ "$ROOT_LOGIN" -eq 1 ]; then
    say "root login (root with this server's root password)"
    install -m 0644 "$HERE/systemd/builtpanel-root-auth.service" /etc/systemd/system/builtpanel-root-auth.service
    systemctl daemon-reload
    systemctl enable builtpanel-root-auth > /dev/null 2>&1
    systemctl restart builtpanel-root-auth
    as_panel bpctl admin root-login --email "root@$HOSTNAME_FQDN" > /dev/null
    # P: root has a usable password; L: locked; NP: none (cloud images often log in with a key only).
    ROOT_PW_STATE=$(passwd -S root 2> /dev/null | awk '{print $2}')
else
    as_panel bpctl admin root-login --disable > /dev/null
    if [ -f /etc/systemd/system/builtpanel-root-auth.service ]; then
        systemctl disable --now builtpanel-root-auth > /dev/null 2>&1 || true
    fi
fi

# --- the service, the web server, the firewall ----------------------------------------------
say "the control plane's service"
install -m 0644 "$HERE/systemd/builtpanel-control-api.service" /etc/systemd/system/builtpanel-control-api.service
systemctl daemon-reload
systemctl enable builtpanel-control-api > /dev/null 2>&1
systemctl restart builtpanel-control-api
for _ in $(seq 1 40); do curl -fsS http://127.0.0.1:8080/api/v1/public/ready > /dev/null 2>&1 && break; sleep 0.5; done
curl -fsS http://127.0.0.1:8080/api/v1/public/ready > /dev/null 2>&1 || die "the control plane does not answer (journalctl -u builtpanel-control-api)"

WEB_ROOT=/usr/share/builtpanel/web
if [ -n "$WEB_DIR" ]; then
    say "the web interface (clients on $HOSTNAME_FQDN, administrators on $ADMIN_HOSTNAME)"
    install -d -m 0755 "$WEB_ROOT"
    for app in client admin; do
        rm -rf "$WEB_ROOT/$app.new"
        if [ "$WEB_FORM" = archives ]; then
            mkdir "$WEB_ROOT/$app.new"
            tar -xzf "$WEB_DIR/web-$app" -C "$WEB_ROOT/$app.new" --no-same-owner --no-same-permissions
        else
            cp -r "$WEB_DIR/$app" "$WEB_ROOT/$app.new"
        fi
        [ -f "$WEB_ROOT/$app.new/index.html" ] || die "the $app web interface has no index.html"
        chown -R root:root "$WEB_ROOT/$app.new"; chmod -R u=rwX,go=rX "$WEB_ROOT/$app.new"
        rm -rf "$WEB_ROOT/$app.old"; [ -d "$WEB_ROOT/$app" ] && mv "$WEB_ROOT/$app" "$WEB_ROOT/$app.old"
        mv "$WEB_ROOT/$app.new" "$WEB_ROOT/$app"; rm -rf "$WEB_ROOT/$app.old"
    done
fi

# One site block of the panel: the API goes to the control plane, the rest is an app's files (any
# path that is not a file is the app's own page: it is a single-page app).
panel_site() { # name, app folder
    cat <<BLOCK
$1 {
    encode zstd gzip
    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
    }
    handle /api/* {
        reverse_proxy 127.0.0.1:8080
    }
    handle {
        root * $WEB_ROOT/$2
        @assets path /assets/*
        header @assets Cache-Control "public, max-age=31536000, immutable"
        @page not path /assets/*
        header @page Cache-Control no-cache
        try_files {path} /index.html
        file_server
    }
}
BLOCK
}

say "Caddy in front of the panel"
{
    options=""
    if [ "$WITH_AGENT" -eq 1 ]; then
        # One web server for the panel and for the sites on this machine. The node agent owns the
        # server named `builtpanel` (it fills it with every site's route), so the panel's own server has
        # that name and the agent is told the panel's names: they stay in what it sends.
        printf '{\n    servers :443 {\n        name builtpanel\n    }\n}\n'
    fi
    if [ -n "$WEB_DIR" ]; then
        panel_site "$HOSTNAME_FQDN" client
        panel_site "$ADMIN_HOSTNAME" admin
    else
        printf '%s {\n    encode zstd gzip\n    reverse_proxy 127.0.0.1:8080\n}\n' "$HOSTNAME_FQDN"
    fi
} > /etc/caddy/Caddyfile
caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile > /dev/null 2>&1 || die "the Caddyfile is not valid (caddy validate --config /etc/caddy/Caddyfile)"
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
    "$HERE/agent/install.sh" --binary "$BIN_DIR/builtpanel-agent" --priv-helper "$BIN_DIR/builtpanel-priv-helper" --rootless-docker
    agent_uid=$(id -u builtpanel-agent)
    if ! grep -qw cpu "/sys/fs/cgroup/user.slice/user-${agent_uid}.slice/user@${agent_uid}.service/cgroup.subtree_control" 2> /dev/null; then
        ui_warn "this server does not hand the CPU controller to the agent's user (a container-type VPS, or an old systemd): hosting accounts cannot get CPU limits here"
        CPU_LIMITS=no
    fi
    # The agent drives the web server on this machine (Caddy's admin API, which listens on this
    # machine only) and keeps the panel's own name in what it sends it.
    install -d -m 0755 /etc/systemd/system/builtpanel-agent.service.d
    cat > /etc/systemd/system/builtpanel-agent.service.d/panel.conf <<CONF
[Service]
Environment=BUILTPANEL_AGENT_CADDY_ADMIN=http://127.0.0.1:2019
Environment=BUILTPANEL_AGENT_PANEL_HOST=$HOSTNAME_FQDN
Environment=BUILTPANEL_AGENT_PANEL_UPSTREAM=127.0.0.1:8080
$([ -n "$WEB_DIR" ] && printf 'Environment=BUILTPANEL_AGENT_PANEL_ADMIN_HOST=%s\nEnvironment=BUILTPANEL_AGENT_PANEL_WEB_ROOT=%s\n' "$ADMIN_HOSTNAME" "$WEB_ROOT")
CONF
    systemctl daemon-reload
    systemctl try-restart builtpanel-agent 2> /dev/null || true
fi

# The administrator with a password of its own (it is the way in when `root` cannot be: root login
# off, or root has no password on this server). Made last, so a password that is printed is never
# lost to an install that stopped half way. With root login on and a root password set, root is
# the administrator, like WHM, and nothing is generated.
say "the first administrator"
ADMIN_PASSWORD=""
if [ "$ROOT_LOGIN" -eq 0 ] || [ "$ROOT_PW_STATE" != P ]; then
    if [ "$(runuser -u postgres -- psql -qAt -d builtpanel -c "select count(*) from accounts where kind = 'admin' and password_hash like '\$argon2id\$%'")" = 0 ]; then
        ADMIN_PASSWORD="$(openssl rand -base64 24 | tr -d '/+=' | cut -c1-24)-Aa1"
        printf '%s\n' "$ADMIN_PASSWORD" | as_panel bpctl admin create --username "$ADMIN_USER" --email "$ADMIN_EMAIL" --password-stdin > /dev/null
    fi
fi

trap - EXIT
ui_done
cat -s <<EOF | ui_box
${UI_OK:-}${UI_B:-}Built Panel is installed.${UI_R:-}

  Panel:     https://$HOSTNAME_FQDN   (the certificate arrives within a minute, once the name points here)
$([ -n "$WEB_DIR" ] && printf '  Admin:     https://%s   (this name must point here too)\n' "$ADMIN_HOSTNAME")
$([ "$ROOT_LOGIN" -eq 1 ] && [ "$ROOT_PW_STATE" = P ] && printf '  Login:     root\n  Password:  this server'"'"'s root password (the one you log in to the server with)\n  Access:    full, like WHM'"'"'s root (role super_admin)\n')
$([ -n "$ADMIN_PASSWORD" ] && printf '  Login:     %s   (role super_admin: every permission on the panel)\n  Password:  %s\n  (shown once, never stored: sign in and change it, then set up two-factor)\n' "$ADMIN_USER" "$ADMIN_PASSWORD")
$([ "$ROOT_LOGIN" -eq 1 ] && [ "$ROOT_PW_STATE" != P ] && printf '  Root:      root has no usable password on this server: set one with `sudo passwd root`, then log in as root\n')
$([ -z "$ADMIN_PASSWORD" ] && [ "$ROOT_LOGIN" -eq 0 ] && echo '  (an administrator already existed; its password is unchanged)')

$([ "$CPU_LIMITS" = no ] && printf 'Note: this server does not hand the CPU controller to the agent, so hosting plans cannot limit CPU here.\n\n')Keep a copy of $SECRETS/master_key somewhere else: without it a lost server cannot be restored
(deploy/runbooks/system-restore.md).
$([ "$WITH_AGENT" -eq 1 ] && printf '\nThe local node is installed but not enrolled: add a node in the panel, then run the enrol command it\nshows as the agent user, then `systemctl enable --now builtpanel-agent` (see deploy/agent/install.sh).\n')
EOF
