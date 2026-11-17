#!/usr/bin/env bash
# 一键启动：安装依赖 → 构建前端 → 以 release 模式运行后端（同时托管前端）
set -euo pipefail
cd "$(dirname "$0")"
[ -d runtime/node_modules ] || (cd runtime && npm install)
[ -d frontend/node_modules ] || (cd frontend && npm install)
(cd frontend && npm run build)
source "$HOME/.cargo/env" 2>/dev/null || true
cd backend && exec cargo run --release
