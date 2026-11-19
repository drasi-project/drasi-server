#!/usr/bin/env bash
# Copyright 2026 The Drasi Authors.
# Licensed under the Apache License, Version 2.0.

set -euo pipefail

usage() {
    printf 'Usage: %s [start|validate|init|build-plugins [http|postgres]|live [prometheus|otlp]|live-postgres [prometheus|otlp]|showcase|send [N]]\n' "$0"
    printf '  start          Run the configuration-only Server in the foreground (default).\n'
    printf '  validate       Validate server-config.yaml without starting a Server.\n'
    printf '  init           Run the wizard, writing wizard-config.yaml without overwriting it.\n'
    printf '  build-plugins  Explicitly build Server + one matched local scenario bundle (default http).\n'
    printf '  live           Run HTTP input -> native query -> log, choosing one metrics backend.\n'
    printf '  live-postgres  Run an existing PostgreSQL CDC source -> native query -> log.\n'
    printf '  showcase       Run the HTTP pipeline with Jaeger, Prometheus and Grafana in podman and steady input.\n'
    printf '  send [N]       Send N reading updates (default 5, at most 1000) to the running demo.\n'
}

if [[ $# -gt 2 ]]; then
    usage >&2
    exit 2
fi

action="${1:-start}"
scenario=http
case "$action" in
    start|validate|init)
        [[ $# -le 1 ]] || { usage >&2; exit 2; }
        ;;
    build-plugins)
        case "${2:-http}" in
            http|postgres) scenario="${2:-http}" ;;
            *) usage >&2; exit 2 ;;
        esac
        ;;
    live|live-postgres)
        [[ "$action" == live-postgres ]] && scenario=postgres
        case "${2:-prometheus}" in
            prometheus|otlp) export DRASI_DEMO_METRICS_BACKEND="${2:-prometheus}" ;;
            *) usage >&2; exit 2 ;;
        esac
        ;;
    showcase)
        [[ $# -le 1 ]] || { usage >&2; exit 2; }
        export DRASI_DEMO_METRICS_BACKEND=prometheus
        ;;
    send)
        count="${2:-5}"
        [[ "$count" =~ ^[1-9][0-9]{0,2}$ || "$count" == 1000 ]] || { usage >&2; exit 2; }
        ;;
    -h|--help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_DIR="$ROOT/target/observability-server"
PLUGINS_DIR="$ROOT/target/observability-example/empty-plugins"
SERVER="$TARGET_DIR/debug/drasi-server"
CONFIG="$SCRIPT_DIR/server-config.yaml"

# showcase: pinned images, demo-owned names and loopback-only ports.
JAEGER_IMAGE=cr.jaegertracing.io/jaegertracing/jaeger:2.22.0
PROMETHEUS_IMAGE=docker.io/prom/prometheus:v3.16.0
GRAFANA_IMAGE=docker.io/grafana/grafana:13.3.1
SHOWCASE=drasi-server-observability-showcase
SHOWCASE_FILES="$SCRIPT_DIR/showcase"
SHOWCASE_LABEL=io.drasi.demo=server-observability-showcase
SERVICE_NAME=drasi-server-observability
DASHBOARD_UID=drasi-server-observability
# Children get their own process group: Ctrl+C reaches only this script,
# which then stops them in order (generator, Server, containers).
NEW_PROCESS_GROUP='import os, sys; os.setpgid(0, 0); os.execvp(sys.argv[1], sys.argv[1:])'
api_port="${DRASI_DEMO_PORT:-18081}"
source_port="${DRASI_DEMO_SOURCE_PORT:-18090}"
otlp_port="${DRASI_DEMO_OTLP_PORT:-4318}"
jaeger_port="${DRASI_DEMO_JAEGER_PORT:-16686}"
prometheus_port="${DRASI_DEMO_PROMETHEUS_PORT:-9090}"
grafana_port="${DRASI_DEMO_GRAFANA_PORT:-3000}"
port_settings=(
    "DRASI_DEMO_PORT=$api_port" "DRASI_DEMO_SOURCE_PORT=$source_port"
    "DRASI_DEMO_OTLP_PORT=$otlp_port" "DRASI_DEMO_JAEGER_PORT=$jaeger_port"
    "DRASI_DEMO_PROMETHEUS_PORT=$prometheus_port" "DRASI_DEMO_GRAFANA_PORT=$grafana_port"
)
showcase_state=""
server_pid=""
traffic_pid=""
network_id=""

# check_ports free: every port is valid, distinct and unused on 127.0.0.1.
# check_ports closed: nothing listens on any of them any more.
check_ports() {
    python3 - "$1" "${port_settings[@]}" <<'PY'
import socket
import sys

mode, settings = sys.argv[1], sys.argv[2:]
seen, busy = set(), []
for setting in settings:
    name, raw = setting.split("=", 1)
    if not raw.isascii() or not raw.isdigit() or not 1 <= int(raw) <= 65535:
        sys.exit(f"{name} must be a TCP port from 1 to 65535.")
    port = int(raw)
    if port in seen:
        sys.exit("The showcase ports must be distinct.")
    seen.add(port)
    with socket.socket() as probe:
        probe.settimeout(1)
        listening = probe.connect_ex(("127.0.0.1", port)) == 0
    if not listening and mode == "free":
        with socket.socket() as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                listener.bind(("127.0.0.1", port))
            except OSError:
                listening = True
    if listening:
        busy.append(f"{name}={port}")
if busy:
    sys.exit(f"Ports {'already in use' if mode == 'free' else 'still open'}: {', '.join(busy)}.")
PY
}

showcase_preflight() {
    local command name
    for command in podman curl; do
        command -v "$command" >/dev/null || {
            printf 'showcase requires %s.\n' "$command" >&2
            exit 1
        }
    done
    if ! podman info >/dev/null 2>&1; then
        printf 'showcase requires a working podman (on macOS: podman machine start).\n' >&2
        exit 1
    fi
    for name in "$SHOWCASE-jaeger" "$SHOWCASE-prometheus" "$SHOWCASE-grafana"; do
        if podman container exists "$name"; then
            printf 'Container %s already exists; refusing to replace it. Remove it yourself if it is a leftover.\n' "$name" >&2
            exit 1
        fi
    done
    if podman network exists "$SHOWCASE"; then
        printf 'Podman network %s already exists; refusing to reuse it. Remove it yourself if it is a leftover.\n' "$SHOWCASE" >&2
        exit 1
    fi
    check_ports free || exit 1
}

http_ok() {
    curl --noproxy '*' --fail --silent --show-error --max-time 3 "$1"
}

# True when the first sample of a Prometheus instant query is above zero.
prometheus_positive() {
    curl --noproxy '*' --fail --silent --show-error --max-time 3 --get \
        --data-urlencode "query=$1" "http://127.0.0.1:$prometheus_port/api/v1/query" |
        python3 -c 'import json, sys; r = json.load(sys.stdin)["data"]["result"]; sys.exit(0 if r and float(r[0]["value"][1]) > 0 else 1)'
}

# Prints the ID of a recent trace that starts with an input at the HTTP source.
recent_input_trace() {
    python3 - "$jaeger_port" "$SERVICE_NAME" <<'PY'
import datetime
import json
import sys
import urllib.parse
import urllib.request

port, service = sys.argv[1], sys.argv[2]
now = datetime.datetime.now(datetime.timezone.utc)
stamp = lambda moment: moment.strftime("%Y-%m-%dT%H:%M:%SZ")
query = urllib.parse.urlencode({
    "query.service_name": service,
    "query.operation_name": "source.publish",
    "query.start_time_min": stamp(now - datetime.timedelta(minutes=5)),
    "query.start_time_max": stamp(now + datetime.timedelta(minutes=1)),
    "query.search_depth": "1",
})
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
with opener.open(f"http://127.0.0.1:{port}/api/v3/traces?{query}", timeout=3) as response:
    result = json.load(response)["result"]
print(result["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["traceId"])
PY
}

wait_for() {
    local description="$1" attempt
    shift
    for ((attempt = 0; attempt < 90; attempt += 1)); do
        if [[ -n "$server_pid" ]] && ! kill -0 "$server_pid" 2>/dev/null; then
            printf 'Server stopped unexpectedly while waiting for %s.\n' "$description" >&2
            exit 1
        fi
        if "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    printf 'Timed out waiting for %s.\n' "$description" >&2
    exit 1
}

showcase_cleanup() {
    local status=$? failed=0 name file cid waited
    # Attempt every step even when output fails, for example after the terminal
    # hung up (EIO) or a reader of a piped stdout exited (EPIPE, not SIGPIPE).
    set +e
    trap - EXIT
    trap '' INT TERM HUP PIPE
    printf '\nStopping the input generator, Server, then the showcase containers and network...\n'
    if [[ -n "$traffic_pid" ]]; then
        kill -TERM "$traffic_pid" 2>/dev/null || true
        wait "$traffic_pid" 2>/dev/null || true
    fi
    if [[ -n "$server_pid" ]]; then
        kill -TERM "$server_pid" 2>/dev/null || true
        for ((waited = 0; waited < 45; waited += 1)); do
            kill -0 "$server_pid" 2>/dev/null || break
            sleep 1
        done
        if kill -0 "$server_pid" 2>/dev/null; then
            printf 'Server did not stop within 45 s; killing it.\n' >&2
            kill -KILL "$server_pid" 2>/dev/null || true
            failed=1
        fi
        wait "$server_pid" 2>/dev/null || printf 'Server exited with status %s.\n' "$?" >&2
    fi
    if [[ "$status" -ne 0 && "$status" -lt 128 && -s "$showcase_state/server.log" ]]; then
        printf 'Last Server log lines:\n' >&2
        tail -n 30 "$showcase_state/server.log" >&2
    fi
    if [[ -n "$showcase_state" ]]; then
        for name in grafana prometheus jaeger; do
            file="$showcase_state/$name.cid"
            [[ -s "$file" ]] || continue
            # `read`, not $(cat ...): a forked substitution can also capture
            # stdout data that a failed printf left buffered, corrupting the ID.
            IFS= read -r cid <"$file" || true
            if [[ "$cid" =~ ^[0-9a-f]{12,64}$ ]] && podman rm --force --volumes --time 5 "$cid" >/dev/null; then
                rm -f -- "$file"
            else
                printf 'Could not remove owned container %s-%s (%s).\n' "$SHOWCASE" "$name" "$cid" >&2
                failed=1
            fi
        done
    fi
    if [[ -n "$network_id" ]]; then
        if podman network rm "$network_id" >/dev/null; then
            network_id=""
        else
            printf 'Could not remove owned podman network %s.\n' "$SHOWCASE" >&2
            failed=1
        fi
    fi
    for ((waited = 0; waited < 15; waited += 1)); do
        check_ports closed 2>/dev/null && break
        sleep 1
    done
    if check_ports closed; then
        printf 'Stopped. Ports closed on 127.0.0.1: %s, %s, %s, %s, %s, %s.\n' "$api_port" \
            "$source_port" "$otlp_port" "$jaeger_port" "$prometheus_port" "$grafana_port"
    else
        failed=1
    fi
    if [[ "$failed" -eq 1 ]]; then
        printf 'Cleanup incomplete; owned state and logs: %s\n' "$showcase_state" >&2
        [[ "$status" -ne 0 ]] || status=1
    fi
    exit "$status"
}

run_showcase() {
    local image grafana_url="http://127.0.0.1:$grafana_port" send_prefix="" trace_id=""
    showcase_state="$RUN_DIR/showcase"
    mkdir "$showcase_state" "$showcase_state/targets"
    trap showcase_cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    printf '[{"targets": ["host.containers.internal:%s"]}]\n' "$api_port" \
        >"$showcase_state/targets/drasi-server.json"
    export DRASI_DEMO_PORT="$api_port" DRASI_DEMO_SOURCE_PORT="$source_port"
    export DRASI_DEMO_PROCESS_METRICS=true
    export DRASI_DEMO_TRACE_ENDPOINT="http://127.0.0.1:$otlp_port/v1/traces"
    printf 'Starting the observability showcase; logs and state: %s\n' "$showcase_state"

    for image in "$JAEGER_IMAGE" "$PROMETHEUS_IMAGE" "$GRAFANA_IMAGE"; do
        if ! podman image exists "$image"; then
            printf 'Pulling %s...\n' "$image"
            podman pull --quiet "$image" >/dev/null
        fi
    done
    podman network create --label "$SHOWCASE_LABEL" "$SHOWCASE" >/dev/null
    network_id="$SHOWCASE"
    network_id="$(podman network inspect --format '{{.Id}}' "$SHOWCASE")"
    podman create --cidfile "$showcase_state/jaeger.cid" --name "$SHOWCASE-jaeger" \
        --label "$SHOWCASE_LABEL" --network "$SHOWCASE" \
        --publish "127.0.0.1:$otlp_port:4318" --publish "127.0.0.1:$jaeger_port:16686" \
        "$JAEGER_IMAGE" >/dev/null
    podman create --cidfile "$showcase_state/prometheus.cid" --name "$SHOWCASE-prometheus" \
        --label "$SHOWCASE_LABEL" --network "$SHOWCASE" \
        --publish "127.0.0.1:$prometheus_port:9090" \
        --volume "$SHOWCASE_FILES/prometheus.yml:/etc/prometheus/prometheus.yml:ro" \
        --volume "$showcase_state/targets:/etc/prometheus/targets:ro" \
        "$PROMETHEUS_IMAGE" --config.file=/etc/prometheus/prometheus.yml \
        --storage.tsdb.path=/prometheus --storage.tsdb.retention.time=1d >/dev/null
    # Anonymous read-only Viewer; no admin user, login form or basic auth.
    podman create --cidfile "$showcase_state/grafana.cid" --name "$SHOWCASE-grafana" \
        --label "$SHOWCASE_LABEL" --network "$SHOWCASE" \
        --publish "127.0.0.1:$grafana_port:3000" \
        --env GF_SERVER_ROOT_URL="$grafana_url/" \
        --env GF_SECURITY_DISABLE_INITIAL_ADMIN_CREATION=true \
        --env GF_AUTH_ANONYMOUS_ENABLED=true --env GF_AUTH_ANONYMOUS_ORG_ROLE=Viewer \
        --env GF_AUTH_DISABLE_LOGIN_FORM=true --env GF_AUTH_BASIC_ENABLED=false \
        --env GF_DASHBOARDS_DEFAULT_HOME_DASHBOARD_PATH="/etc/grafana/dashboards/$DASHBOARD_UID.json" \
        --env GF_ANALYTICS_REPORTING_ENABLED=false --env GF_ANALYTICS_CHECK_FOR_UPDATES=false \
        --env GF_ANALYTICS_CHECK_FOR_PLUGIN_UPDATES=false --env GF_NEWS_NEWS_FEED_ENABLED=false \
        --env GF_PLUGINS_PREINSTALL_DISABLED=true \
        --volume "$SHOWCASE_FILES/grafana/provisioning/datasources:/etc/grafana/provisioning/datasources:ro" \
        --volume "$SHOWCASE_FILES/grafana/provisioning/dashboards:/etc/grafana/provisioning/dashboards:ro" \
        --volume "$SHOWCASE_FILES/grafana/dashboards:/etc/grafana/dashboards:ro" \
        "$GRAFANA_IMAGE" >/dev/null
    for name in jaeger prometheus grafana; do
        podman start "$(cat "$showcase_state/$name.cid")" >/dev/null
    done
    wait_for Jaeger http_ok "http://127.0.0.1:$jaeger_port/api/v3/services"

    (cd "$RUN_DIR" && exec python3 -c "$NEW_PROCESS_GROUP" "$SERVER" \
        --config "$CONFIG_FILE" --plugins-dir "$RUN_DIR/plugins" \
        --disable-ui --skip-verification) >"$showcase_state/server.log" 2>&1 &
    server_pid=$!
    wait_for 'Server management API' http_ok "http://127.0.0.1:$api_port/health"
    wait_for 'HTTP source' http_ok "http://127.0.0.1:$source_port/health"
    python3 -c "$NEW_PROCESS_GROUP" python3 "$SHOWCASE_FILES/traffic.py" trickle \
        --port "$api_port" --instance "$DRASI_DEMO_INSTANCE" --interval 2 \
        >"$showcase_state/traffic.log" 2>&1 &
    traffic_pid=$!

    wait_for 'Prometheus' http_ok "http://127.0.0.1:$prometheus_port/-/ready"
    wait_for 'Prometheus to scrape Server' prometheus_positive 'up{job="drasi-server"}'
    wait_for 'scraped pipeline input' prometheus_positive 'sum(drasi_source_events_dispatched)'
    wait_for 'an input trace in Jaeger' recent_input_trace
    trace_id="$(recent_input_trace)" || trace_id=""
    [[ "$trace_id" =~ ^[0-9a-f]{32}$ ]] || trace_id=""
    wait_for 'Grafana' http_ok "$grafana_url/api/health"
    wait_for 'the Grafana dashboard' http_ok "$grafana_url/api/dashboards/uid/$DASHBOARD_UID"

    [[ "$api_port" == 18081 ]] || send_prefix="DRASI_DEMO_PORT=$api_port "
    # The dashboard's Traces row opens on one real input trace; paste others from Jaeger.
    printf '\nGrafana dashboard: %s/d/%s%s\n' "$grafana_url" "$DASHBOARD_UID" \
        "${trace_id:+?var-trace_id=$trace_id}"
    printf 'Jaeger traces:     http://127.0.0.1:%s/search?service=%s\n' "$jaeger_port" "$SERVICE_NAME"
    printf 'Prometheus:        http://127.0.0.1:%s/targets\n' "$prometheus_port"
    printf 'More input:        %sbash examples/observability/demo.sh send 20\n' "$send_prefix"
    printf '\nDemo is live. Press Ctrl+C to stop and clean up.\n'

    # Returns early when a trapped signal arrives; the EXIT trap then cleans up.
    if wait "$server_pid"; then
        printf 'Server exited unexpectedly.\n' >&2
    else
        printf 'Server exited unexpectedly with status %s.\n' "$?" >&2
    fi
    server_pid=""
    exit 1
}

cd "$ROOT"
if [[ "$action" == build-plugins ]]; then
    exec python3 "$SCRIPT_DIR/local_bundle.py" build "$scenario"
fi
if [[ "$action" == send ]]; then
    exec python3 "$SHOWCASE_FILES/traffic.py" send --port "$api_port" --count "$count"
fi
if [[ "$action" == live || "$action" == live-postgres || "$action" == showcase ]]; then
    if [[ "${DRASI_DEMO_TRUST_LOCAL_PLUGINS:-}" != 1 ]]; then
        printf '%s requires DRASI_DEMO_TRUST_LOCAL_PLUGINS=1 to trust the three receipt-checked, unsigned local demo plugins.\n' "$action" >&2
        exit 2
    fi
    if [[ "$scenario" == postgres ]]; then
        # The launcher never starts or chooses a database; supply an isolated one.
        for name in DRASI_DEMO_PG_HOST DRASI_DEMO_PG_DATABASE DRASI_DEMO_PG_USER DRASI_DEMO_PG_PASSWORD; do
            if [[ -z "${!name:-}" ]]; then
                printf 'live-postgres requires %s for an existing, isolated PostgreSQL database.\n' "$name" >&2
                exit 2
            fi
        done
        CONFIG_FILE="$SCRIPT_DIR/postgres-config.yaml"
    else
        CONFIG_FILE="$SCRIPT_DIR/drasi-server.yaml"
    fi
    default_profile=persistence
    if [[ "$action" == showcase ]]; then
        default_profile=debug
    fi
    export DRASI_DEMO_PROFILE="${DRASI_DEMO_PROFILE:-$default_profile}"
    case "$DRASI_DEMO_PROFILE" in
        basic|debug|persistence) ;;
        *) printf '%s requires basic, debug or persistence; use start for Off.\n' "${action%-postgres}" >&2; exit 2 ;;
    esac
    if [[ "$action" == showcase ]]; then
        showcase_preflight
    fi
    RUN_DIR="$(python3 "$SCRIPT_DIR/local_bundle.py" prepare "$scenario")"
    export DRASI_DEMO_INSTANCE="${RUN_DIR##*/}"
    if [[ "$action" == showcase ]]; then
        run_showcase
    fi
    printf 'Live %s %s pipeline; instance=%s\nOwned WAL/state directory (retained): %s\n' \
        "$scenario" "$DRASI_DEMO_METRICS_BACKEND" "$DRASI_DEMO_INSTANCE" "$RUN_DIR"
    printf 'Management: http://127.0.0.1:%s ; Ctrl+C stops the foreground Server.\n' \
        "${DRASI_DEMO_PORT:-18081}"
    if [[ -n "${DRASI_DEMO_TRACE_ENDPOINT:-}" ]]; then
        printf 'Trace export: HTTP/protobuf to the configured receiver; sample ratio %s.\n' \
            "${DRASI_DEMO_SAMPLE_RATIO:-1.0}"
    else
        printf 'Trace export: disabled (set DRASI_DEMO_TRACE_ENDPOINT to a full /v1/traces URL).\n'
    fi
    cd "$RUN_DIR"
    exec "$SERVER" --config "$CONFIG_FILE" \
        --plugins-dir "$RUN_DIR/plugins" --disable-ui --skip-verification
fi
cargo build --offline --locked --bin drasi-server --target-dir "$TARGET_DIR"
mkdir -p "$PLUGINS_DIR"

case "$action" in
    start)
        printf 'Starting a configuration-only Server; no data pipeline or telemetry export. Ctrl+C stops it.\n'
        exec "$SERVER" --config "$CONFIG" --plugins-dir "$PLUGINS_DIR" --disable-ui
        ;;
    validate)
        exec "$SERVER" validate --config "$CONFIG" --plugins-dir "$PLUGINS_DIR"
        ;;
    init)
        exec "$SERVER" init --output "$SCRIPT_DIR/wizard-config.yaml" --plugins-dir "$PLUGINS_DIR"
        ;;
esac
