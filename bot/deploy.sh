#!/usr/bin/env bash
# Разворачивает бота на чистой Ubuntu. Запускать от root: bash deploy.sh
set -euo pipefail

APP_DIR=/opt/signum-bot
IPATOOL_URL=${IPATOOL_URL:-}

apt-get update
# fonts-dejavu-core нужен картинкам каталога: без шрифта подписи не нарисуются
apt-get install -y python3-venv python3-pip nginx certbot python3-certbot-nginx fonts-dejavu-core

id -u signum >/dev/null 2>&1 || useradd --system --home "$APP_DIR" --shell /usr/sbin/nologin signum
mkdir -p "$APP_DIR"/{data,bin}

# код
cp -r signum_bot "$APP_DIR"/
cp requirements.txt "$APP_DIR"/
[ -f "$APP_DIR/.env" ] || cp .env.example "$APP_DIR/.env"

# ipatool: положите бинарник рядом (bin/ipatool) или задайте IPATOOL_URL
if [ -n "$IPATOOL_URL" ]; then
  curl -fsSL "$IPATOOL_URL" -o "$APP_DIR/bin/ipatool"
elif [ -f bin/ipatool ]; then
  cp bin/ipatool "$APP_DIR/bin/ipatool"
elif [ -f ../linux_x64/ipatool-v6 ]; then
  cp ../linux_x64/ipatool-v6 "$APP_DIR/bin/ipatool"
elif [ -f ../linux_x64/ipatool-v5 ]; then
  # Signum's v5 directory contains upstream ipatool 1.0.7, also used by v6.
  cp ../linux_x64/ipatool-v5 "$APP_DIR/bin/ipatool"
else
  echo "!! Нужен ipatool 1.0.7: положите bin/ipatool или задайте IPATOOL_URL"
  exit 1
fi
for metadata in bin/SOURCE.json bin/LICENSE.ipatool; do
  [ ! -f "$metadata" ] || cp "$metadata" "$APP_DIR/bin/"
done
chmod +x "$APP_DIR/bin/ipatool" 2>/dev/null || echo "!! положите ipatool в $APP_DIR/bin/ipatool"

python3 -m venv "$APP_DIR/venv"
"$APP_DIR/venv/bin/pip" install -U pip -r "$APP_DIR/requirements.txt"

chown -R signum:signum "$APP_DIR"
chmod 600 "$APP_DIR/.env"

cp signum-bot.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now signum-bot

echo
echo "Готово. Дальше:"
echo "  1. nano $APP_DIR/.env    — BOT_ADMINS, PUBLIC_BASE_URL"
echo "  2. настройте nginx на домен и получите TLS: certbot --nginx -d bot.example.com"
echo "  3. systemctl restart signum-bot && journalctl -u signum-bot -f"
