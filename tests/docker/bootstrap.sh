#!/usr/bin/env bash
#
# Provision a test identity and two test services against the
# docker-compose Ziti network started by tests/docker/docker-compose.yml,
# then write an SDK-compatible identity bundle to ./test-identity.json.
#
# The standard `ziti edge enroll` output stores the cert/key/CA as PEM
# strings inline in the JSON. Our `Context::from_file` expects file
# paths instead, so this script splits the PEMs into sibling files and
# rewrites the JSON to point at them.
#
# Usage: tests/docker/bootstrap.sh
#
# Requires: docker, jq, the ziti-quickstart container running.

set -euo pipefail

CONTAINER=ziti-quickstart
# Inside the container the controller is reachable as ziti-controller
# (a network alias on the quickstart bridge). The runner host reaches
# it via the published 1280 port on localhost — the CI workflow adds a
# /etc/hosts entry so `ziti-controller` resolves to 127.0.0.1 there.
CTRL_URL=${CTRL_URL:-https://ziti-controller:1280}
OUT_DIR=${OUT_DIR:-"$(cd "$(dirname "$0")" && pwd)"}
IDENTITY_NAME=${IDENTITY_NAME:-test-user}
DIAL_SERVICE=${DIAL_SERVICE:-echo-service}
LISTEN_SERVICE=${LISTEN_SERVICE:-test-listen-service}

log() { printf '[bootstrap] %s\n' "$*" >&2; }

ziti_in() { docker exec "$CONTAINER" ziti "$@"; }

log "waiting for controller readiness at $CTRL_URL"
for _ in $(seq 1 60); do
    if curl -skf "$CTRL_URL/edge/management/v1/version" >/dev/null; then
        break
    fi
    sleep 2
done

log "logging in to controller"
ziti_in edge login "$CTRL_URL" -u admin -p admin -y

log "creating dial + bind service-policies (idempotent)"
ziti_in edge create service-policy "${DIAL_SERVICE}-dial" Dial \
    --identity-roles "@$IDENTITY_NAME" \
    --service-roles "@$DIAL_SERVICE" || true
ziti_in edge create service-policy "${LISTEN_SERVICE}-dial" Dial \
    --identity-roles "@$IDENTITY_NAME" \
    --service-roles "@$LISTEN_SERVICE" || true
ziti_in edge create service-policy "${LISTEN_SERVICE}-bind" Bind \
    --identity-roles "@$IDENTITY_NAME" \
    --service-roles "@$LISTEN_SERVICE" || true

log "creating services (idempotent)"
ziti_in edge create service "$DIAL_SERVICE" || true
ziti_in edge create service "$LISTEN_SERVICE" || true

log "creating identity $IDENTITY_NAME"
JWT_PATH=/tmp/${IDENTITY_NAME}.jwt
ziti_in edge delete identity "$IDENTITY_NAME" || true
ziti_in edge create identity device "$IDENTITY_NAME" \
    --jwt-output-file "$JWT_PATH"

log "enrolling identity → standard Ziti JSON"
STD_JSON_IN=/tmp/${IDENTITY_NAME}.json
ziti_in edge enroll "$JWT_PATH" --out "$STD_JSON_IN"

log "copying enrolled JSON out of container"
STD_JSON_OUT="$OUT_DIR/test-identity.raw.json"
docker cp "$CONTAINER:$STD_JSON_IN" "$STD_JSON_OUT"

log "splitting PEM-inline JSON into separate files for our loader"
CERTS_DIR="$OUT_DIR/test-identity-pems"
rm -rf "$CERTS_DIR"
mkdir -p "$CERTS_DIR"

extract_pem() {
    local field=$1
    local out=$2
    local val
    val=$(jq -r ".id.$field // empty" "$STD_JSON_OUT")
    if [[ -z $val ]]; then
        log "ERROR: $field missing from enrolled identity JSON"
        exit 1
    fi
    case "$val" in
        pem:*) printf '%s' "${val#pem:}" >"$out" ;;
        *) printf '%s' "$val" >"$out" ;;
    esac
}

extract_pem cert "$CERTS_DIR/cert.pem"
extract_pem key  "$CERTS_DIR/key.pem"
extract_pem ca   "$CERTS_DIR/ca.pem"

ZT_API=$(jq -r '.ztAPI' "$STD_JSON_OUT")

jq -n \
    --arg ztAPI "$ZT_API" \
    --arg id "$IDENTITY_NAME" \
    --arg cert "$CERTS_DIR/cert.pem" \
    --arg key "$CERTS_DIR/key.pem" \
    --arg ca "$CERTS_DIR/ca.pem" \
    '{ztAPI: $ztAPI, id: $id, cert: $cert, key: $key, ca: $ca}' \
    >"$OUT_DIR/test-identity.json"

log "bootstrap complete"
log "  identity: $OUT_DIR/test-identity.json"
log "  PEMs:     $CERTS_DIR/"
