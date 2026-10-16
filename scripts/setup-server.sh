#!/usr/bin/env bash
#
# One-time provisioning for the Akidev server. Run as root on a fresh
# Debian/Ubuntu host, over your own SSH access.
#
# Installs Docker from the official apt repository, creates an unprivileged deploy
# user, restricts the firewall, and tunes the kernel for many concurrent sockets.
# Safe to re-run.
#
# Usage, from your machine. Prefix with sudo unless you are logging in as root:
#   ssh root@SERVER 'bash -s' < scripts/setup-server.sh
#   ssh ubuntu@SERVER 'sudo bash -s' < scripts/setup-server.sh
#
# Lightsail (and most cloud images) give you an unprivileged user with sudo rather
# than a root login, which is why the sudo form above is the common one.
#
# Or, to install a deploy key in the same step:
#   ssh ubuntu@SERVER "DEPLOY_PUBLIC_KEY='$(cat ~/.ssh/konflix_deploy.pub)' sudo -E bash -s" < scripts/setup-server.sh
#
# Afterwards only SSH and the web ports (80 and 443, which Caddy serves) are reachable.

set -euo pipefail

DEPLOY_USER="${DEPLOY_USER:-konflix}"
APP_DIR="${APP_DIR:-/opt/konflix}"
DEPLOY_PUBLIC_KEY="${DEPLOY_PUBLIC_KEY:-}"
# Must match the SSH port used to reach the host, or the firewall locks it out.
SSH_PORT="${SSH_PORT:-22}"

if [ "$(id -u)" -ne 0 ]; then
  echo "Error: this must run as root. Prefix the command with sudo:" >&2
  echo "  ssh <user>@<host> 'sudo bash -s' < scripts/setup-server.sh" >&2
  exit 1
fi

echo "==> Installing Docker from the official apt repository"
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates curl gnupg rsync ufw jq

# The distribution is read from the host rather than assumed. Both the key URL and
# the repository URL are distribution-specific: the Ubuntu key served from
# /linux/debian/gpg has a uid of 'Docker Release (CE deb) <docker@docker.com>' and
# apt rejects it with NO_PUBKEY for the Ubuntu archive. Verified on Ubuntu 24.04.
. /etc/os-release
DOCKER_DISTRO="$ID"
case "$DOCKER_DISTRO" in
  debian|ubuntu) ;;
  *) echo "Error: unsupported distribution '$DOCKER_DISTRO'." >&2; exit 1 ;;
esac

install -m 0755 -d /etc/apt/keyrings
if [ ! -f /etc/apt/keyrings/docker.asc ]; then
  curl -fsSL "https://download.docker.com/linux/$DOCKER_DISTRO/gpg" -o /etc/apt/keyrings/docker.asc
  chmod a+r /etc/apt/keyrings/docker.asc
fi

echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/$DOCKER_DISTRO $VERSION_CODENAME stable" \
  > /etc/apt/sources.list.d/docker.list

apt-get update
apt-get install -y --no-install-recommends \
  docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# Captured before installing so the daemon is only restarted when Docker was not
# already present. On a re-run over an existing install the restart would interrupt
# any running container, which is not something a provisioning script should do.
DOCKER_WAS_INSTALLED=0
command -v docker >/dev/null 2>&1 && DOCKER_WAS_INSTALLED=1

echo "==> Creating the deploy user"
if ! id -u "$DEPLOY_USER" >/dev/null 2>&1; then
  adduser --disabled-password --gecos "" "$DEPLOY_USER"
fi
# Membership of the docker group is equivalent to root on this host. That is
# acceptable because it is the only privileged role here and the account is key-only.
usermod -aG docker "$DEPLOY_USER"

if [ -n "$DEPLOY_PUBLIC_KEY" ]; then
  echo "==> Installing the deploy public key"
  install -d -m 0700 -o "$DEPLOY_USER" -g "$DEPLOY_USER" "/home/$DEPLOY_USER/.ssh"
  KEY_FILE="/home/$DEPLOY_USER/.ssh/authorized_keys"
  touch "$KEY_FILE"
  # Only add it if it is not already there, so re-running does not stack duplicates.
  if ! grep -qF "$DEPLOY_PUBLIC_KEY" "$KEY_FILE" 2>/dev/null; then
    printf '%s\n' "$DEPLOY_PUBLIC_KEY" >> "$KEY_FILE"
  fi
  chmod 0600 "$KEY_FILE"
  chown "$DEPLOY_USER:$DEPLOY_USER" "$KEY_FILE"
  # The account has no password, so password auth cannot be used against it.
  passwd -l "$DEPLOY_USER" >/dev/null
fi

echo "==> Hardening SSH"
# Key-only, no root login. The deploy key authenticates as $DEPLOY_USER, which owns
# the stack; root is reachable only through the provider console.
#
# This disables password authentication, so it must not run before key access is
# known to work. The check below refuses the common way to lock yourself out: running
# this over an SSH session that authenticated with a password.
if [ -n "${SSH_CONNECTION:-}" ] && [ -z "$DEPLOY_PUBLIC_KEY" ]; then
  echo
  echo "WARNING: this session is over SSH and no DEPLOY_PUBLIC_KEY was provided, so"
  echo "password authentication is about to be disabled with no key installed for"
  echo "$DEPLOY_USER. Existing root key access keeps working, but if this session used"
  echo "a password you will not be able to log in again."
  echo
  echo "Set SSH_DISABLE_PASSWORD=1 to proceed anyway."
  if [ -z "${SSH_DISABLE_PASSWORD:-}" ]; then
    exit 1
  fi
fi

cat > /etc/ssh/sshd_config.d/99-konflix.conf <<CONF
PasswordAuthentication no
PermitRootLogin prohibit-password
CONF
systemctl reload ssh 2>/dev/null || systemctl reload sshd 2>/dev/null || true

echo "==> Restricting the firewall"
# SSH, and the two ports Caddy serves on: 80 for the certificate check and the redirect to https,
# 443 for everything else.
ufw --force reset >/dev/null
ufw default deny incoming >/dev/null
ufw default allow outgoing >/dev/null
ufw allow "$SSH_PORT/tcp" >/dev/null
ufw allow 80/tcp >/dev/null
ufw allow 443/tcp >/dev/null
ufw --force enable >/dev/null

echo "==> Tuning the kernel for many concurrent sockets"
# A viewer holds one connection for the length of an episode, and each active torrent
# opens many sockets of its own. The default open-file limit is the first thing to run
# out. swappiness is lowered because the torrent cache is disposable.
cat > /etc/sysctl.d/99-konflix.conf <<'CONF'
fs.file-max = 1000000
net.core.somaxconn = 4096
net.ipv4.tcp_fin_timeout = 15
vm.swappiness = 10
CONF
sysctl --system >/dev/null

mkdir -p /etc/systemd/system/docker.service.d
cat > /etc/systemd/system/docker.service.d/limits.conf <<'CONF'
[Service]
LimitNOFILE=1000000
CONF
systemctl daemon-reload
if [ "$DOCKER_WAS_INSTALLED" -eq 0 ]; then
  # A fresh install, so there is nothing running to interrupt.
  systemctl restart docker
else
  # The limit is applied by the next daemon restart rather than one forced now.
  echo "    docker already installed; LimitNOFILE applies on its next restart"
fi

echo "==> Creating the application directory"
install -d -m 0755 -o "$DEPLOY_USER" -g "$DEPLOY_USER" "$APP_DIR"

echo
echo "Done."
echo
echo "Next:"
echo "  1. docker version              # confirm the daemon is up"
echo "  2. From your machine, add the private key to GitHub secrets and run the deploy workflow."
echo
echo "Note: membership of the 'docker' group is root-equivalent. This account is"
echo "key-only and is the intended deploy identity."
