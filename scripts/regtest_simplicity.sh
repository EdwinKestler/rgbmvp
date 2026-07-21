#!/usr/bin/env bash
# Manage the P2 Elements-with-Simplicity regtest node.
# Usage: ./scripts/regtest_simplicity.sh {up|down|status|cli|logs|wait}
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

COMPOSE=(docker compose -f "$ROOT/docker-compose.yml")
RPC_HOST="${ELEMENTS_SIMPLICITY_RPC_HOST:-127.0.0.1}"
RPC_PORT="${ELEMENTS_SIMPLICITY_RPC_PORT:-7042}"
RPC_USER="${ELEMENTS_RPC_USER:-user}"
RPC_PASS="${ELEMENTS_RPC_PASSWORD:-pass}"

cli() {
  docker exec rgbmvp-elementsd-simplicity \
    elements-cli -chain=elementsregtest \
    -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASS" -rpcport="$RPC_PORT" \
    "$@"
}

wait_healthy() {
  local tries="${1:-60}"
  local i=0
  echo "waiting for rgbmvp-elementsd-simplicity (port $RPC_PORT)…"
  while (( i < tries )); do
    if docker inspect -f '{{.State.Health.Status}}' rgbmvp-elementsd-simplicity 2>/dev/null | grep -q healthy; then
      echo "healthy"
      return 0
    fi
    # healthcheck may lag; try RPC directly
    if docker exec rgbmvp-elementsd-simplicity \
      elements-cli -chain=elementsregtest -rpcuser="$RPC_USER" -rpcpassword="$RPC_PASS" -rpcport="$RPC_PORT" \
      getblockchaininfo >/dev/null 2>&1; then
      echo "rpc-ready"
      return 0
    fi
    sleep 1
    i=$((i + 1))
  done
  echo "timeout waiting for elementsd-simplicity" >&2
  "${COMPOSE[@]}" logs --tail 40 elementsd-simplicity || true
  return 1
}

cmd="${1:-status}"
shift || true

case "$cmd" in
  up)
    "${COMPOSE[@]}" up -d elementsd-simplicity
    wait_healthy 90
    cli getblockchaininfo | head -c 400
    echo
    echo "RPC: http://${RPC_HOST}:${RPC_PORT}  user=${RPC_USER}  (regtest only)"
    echo "Simplicity: evbparams=simplicity:-1::: (see docker/elements-simplicity.conf)"
    ;;
  down)
    "${COMPOSE[@]}" stop elementsd-simplicity 2>/dev/null || true
    "${COMPOSE[@]}" rm -f elementsd-simplicity 2>/dev/null || true
    echo "stopped elementsd-simplicity (volume kept; use 'wipe' to drop data)"
    ;;
  wipe)
    "${COMPOSE[@]}" down -v --remove-orphans
    echo "all compose services + volumes removed"
    ;;
  status)
    docker ps --filter name=rgbmvp-elementsd-simplicity --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}' || true
    if docker exec rgbmvp-elementsd-simplicity true 2>/dev/null; then
      echo "--- getblockchaininfo ---"
      cli getblockchaininfo
      echo "--- softforks / deployments (grep simplicity) ---"
      cli getblockchaininfo 2>/dev/null | grep -i simplicity || echo "(no simplicity string in tip info; check conf evbparams)"
    else
      echo "container not running — try: $0 up"
      exit 1
    fi
    ;;
  wait)
    wait_healthy "${1:-60}"
    ;;
  cli)
    cli "$@"
    ;;
  logs)
    "${COMPOSE[@]}" logs --tail "${1:-80}" -f elementsd-simplicity
    ;;
  *)
    cat <<EOF
Usage: $0 {up|down|wipe|status|wait|cli|logs}

  up      Start Elements 23.3 Simplicity regtest (host port 7042)
  down    Stop container (keep volume)
  wipe    docker compose down -v (destroy regtest state)
  status  Health + getblockchaininfo
  wait    Block until healthy
  cli …   elements-cli passthrough inside container
  logs    Follow container logs

Pins: docs/P2_SIMPLICITY.md
EOF
    exit 1
    ;;
esac
