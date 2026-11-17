#!/bin/bash
# Persistent reverse tunnel: VPS 127.0.0.1:${REMOTE_PORT} -> local 127.0.0.1:${LOCAL_PORT}.
# Configure via environment variables or data/tunnel.env (git-ignored):
#   VPS_HOST=1.2.3.4  VPS_USER=deploy  VPS_SSH_PORT=22  VPS_SSH_KEY=~/.ssh/id_ed25519
cd "$(dirname "$0")/.." || exit 1
set -a; [ -f data/tunnel.env ] && . data/tunnel.env; set +a
: "${VPS_HOST:?set VPS_HOST}" "${VPS_USER:?set VPS_USER}"
VPS_SSH_PORT="${VPS_SSH_PORT:-22}"
VPS_SSH_KEY="${VPS_SSH_KEY:-$HOME/.ssh/id_ed25519}"
REMOTE_PORT="${REMOTE_PORT:-18787}"
LOCAL_PORT="${LOCAL_PORT:-8787}"
export AUTOSSH_GATETIME=0
exec autossh -M 0 -N \
  -o ServerAliveInterval=30 -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=yes \
  -o IdentitiesOnly=yes -o BatchMode=yes \
  -i "$VPS_SSH_KEY" \
  -p "$VPS_SSH_PORT" \
  -R "127.0.0.1:${REMOTE_PORT}:127.0.0.1:${LOCAL_PORT}" \
  "${VPS_USER}@${VPS_HOST}"
