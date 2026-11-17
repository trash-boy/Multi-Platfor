#!/bin/bash
# Launched by com.mpf.server. Runs the release server with env (password) from data/server.env.
cd "$(dirname "$0")/.." || exit 1
export PATH="/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
set -a; [ -f data/server.env ] && . data/server.env; set +a
exec ./backend/target/release/mpf-server
