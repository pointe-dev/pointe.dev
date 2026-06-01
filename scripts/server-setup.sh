#!/usr/bin/env bash
# Run once on a fresh Hetzner server as root.
# Tested on Ubuntu 24.04 LTS (CX23).
set -euo pipefail

APP_DIR="/opt/pointe"
DEPLOY_USER="pointe"

echo "=== 1/7 System update ==="
apt-get update -qq && apt-get upgrade -y -qq

echo "=== 2/7 Docker ==="
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
    | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
    https://download.docker.com/linux/ubuntu \
    $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
    | tee /etc/apt/sources.list.d/docker.list > /dev/null
apt-get update -qq
apt-get install -y docker-ce docker-ce-cli containerd.io \
                   docker-buildx-plugin docker-compose-plugin

echo "=== 3/7 Deploy user ==="
useradd -m -s /bin/bash "$DEPLOY_USER" 2>/dev/null || true
usermod -aG docker "$DEPLOY_USER"
mkdir -p /home/"$DEPLOY_USER"/.ssh
chmod 700 /home/"$DEPLOY_USER"/.ssh
chown -R "$DEPLOY_USER":"$DEPLOY_USER" /home/"$DEPLOY_USER"/.ssh

echo "=== 4/7 App directory ==="
mkdir -p "$APP_DIR/secrets"
chown -R "$DEPLOY_USER":"$DEPLOY_USER" "$APP_DIR"

echo "=== 5/7 Firewall ==="
ufw --force reset
ufw default deny incoming
ufw default allow outgoing
ufw allow ssh
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp   # HTTP/3 QUIC
ufw --force enable

echo "=== 6/7 SSH key for GitHub Actions ==="
echo ""
echo "Generate a keypair on your local machine:"
echo "  ssh-keygen -t ed25519 -C 'github-actions-pointe' -f ~/.ssh/pointe_deploy"
echo ""
echo "Add the PUBLIC key to authorized_keys on this server:"
echo "  ssh-copy-id -i ~/.ssh/pointe_deploy.pub $DEPLOY_USER@<THIS_SERVER_IP>"
echo ""
echo "Add the PRIVATE key as secret HETZNER_SSH_KEY in your GitHub repo."

echo "=== 7/7 Required files on server ==="
echo ""
echo "Create the following files in $APP_DIR before running docker compose:"
echo ""
echo "  $APP_DIR/secrets/pg_password.txt   — Postgres password (no newline)"
echo "  $APP_DIR/secrets/session_secret.txt — Session HMAC secret (no newline)"
echo "  $APP_DIR/secrets/admin_ingest_token.txt — Admin token for template ingest"
echo "  $APP_DIR/.env.prod                 — Backend env vars (see below)"
echo "  $APP_DIR/.env.n8n                  — n8n env vars (see below)"
echo ""
echo ".env.prod:"
cat <<'EOF'
ANTHROPIC_API_KEY=sk-ant-...
DATABASE_URL=postgresql://pointe:<PG_PASSWORD>@postgres:5432/pointe
SESSION_SECRET_FILE=/run/secrets/session_secret
ADMIN_INGEST_TOKEN_FILE=/run/secrets/admin_ingest_token
RESEND_API_KEY=re_...
BASE_URL=https://go.pointe.dev
OWNER_EMAIL=your@email.com
STRIPE_SECRET_KEY=sk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...
LANGFUSE_PUBLIC_KEY=pk-lf-...
LANGFUSE_SECRET_KEY=sk-lf-...
LANGFUSE_BASE_URL=https://cloud.langfuse.com
EOF
echo ""
echo ".env.n8n:"
cat <<'EOF'
N8N_ENCRYPTION_KEY=<random-32-char-hex>
N8N_USER_MANAGEMENT_JWT_SECRET=<random-32-char-hex>
EOF
echo ""
echo "Then run: docker compose -f docker-compose.prod.yml up -d"
echo ""
echo "✅ Server ready."
