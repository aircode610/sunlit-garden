#!/usr/bin/env bash
# Create .env for local development from .env.example, filling every secret with a fresh
# random value. Safe to re-run: it never overwrites an existing .env.
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -f .env ]]; then
  echo ".env already exists; leaving it alone (delete it to regenerate)."
  exit 0
fi

hex() { openssl rand -hex "$1"; }

postgres_password="$(hex 24)"

sed \
  -e "s/^POSTGRES_PASSWORD=__GENERATE__/POSTGRES_PASSWORD=${postgres_password}/" \
  -e "s/__POSTGRES_PASSWORD__/${postgres_password}/" \
  -e "s/^S3_ACCESS_KEY_ID=__GENERATE__/S3_ACCESS_KEY_ID=GK$(hex 16)/" \
  -e "s/^S3_SECRET_ACCESS_KEY=__GENERATE__/S3_SECRET_ACCESS_KEY=$(hex 32)/" \
  -e "s/^GARAGE_RPC_SECRET=__GENERATE__/GARAGE_RPC_SECRET=$(hex 32)/" \
  -e "s/^GARAGE_ADMIN_TOKEN=__GENERATE__/GARAGE_ADMIN_TOKEN=$(hex 32)/" \
  -e "s/^GARAGE_METRICS_TOKEN=__GENERATE__/GARAGE_METRICS_TOKEN=$(hex 32)/" \
  .env.example > .env

chmod 600 .env
echo "Created .env with generated secrets. Next: docker compose up -d"
