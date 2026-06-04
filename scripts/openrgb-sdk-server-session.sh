#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/openrgb-sdk-server-session.sh <status|start|stop|restart> [options]

Manage a user-session OpenRGB SDK server for RatVantage keyboard RGB.
This helper runs OpenRGB as the desktop user, not as root.

Options:
  --openrgb-bin <path>    OpenRGB binary. Default: openrgb from PATH.
  --host <host>           SDK host. Default: 127.0.0.1.
  --port <port>           SDK port. Default: 6742.
  --stability-wait <sec>  Seconds the listener must remain alive. Default: 2.
  -h, --help              Show this help.
EOF
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 2
fi

action="$1"
shift
openrgb_bin="openrgb"
host="127.0.0.1"
port="6742"
stability_wait="2"

while (($#)); do
  case "$1" in
    --openrgb-bin)
      openrgb_bin="${2:?missing value for --openrgb-bin}"
      shift 2
      ;;
    --host)
      host="${2:?missing value for --host}"
      shift 2
      ;;
    --port)
      port="${2:?missing value for --port}"
      shift 2
      ;;
    --stability-wait)
      stability_wait="${2:?missing value for --stability-wait}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$action" in
  status|start|stop|restart) ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    echo "unsupported action: $action" >&2
    usage >&2
    exit 2
    ;;
esac

runtime_dir="${XDG_RUNTIME_DIR:-$HOME/.cache}/ratvantage"
cache_dir="$HOME/.cache/ratvantage"
pid_file="$runtime_dir/openrgb-sdk-server-${port}.pid"
stdout_log="$cache_dir/openrgb-sdk-server-${port}.stdout"
stderr_log="$cache_dir/openrgb-sdk-server-${port}.stderr"
mkdir -p "$runtime_dir" "$cache_dir"

socket_ready() {
  python3 - "$port" <<'PY'
import pathlib
import sys

port_hex = f"{int(sys.argv[1]):04X}"
for proc_file in ("/proc/net/tcp", "/proc/net/tcp6"):
    path = pathlib.Path(proc_file)
    if not path.exists():
        continue
    for line in path.read_text().splitlines()[1:]:
        parts = line.split()
        if len(parts) < 4:
            continue
        local = parts[1]
        state = parts[3]
        if local.rsplit(":", 1)[-1].upper() == port_hex and state == "0A":
            raise SystemExit(0)
raise SystemExit(1)
PY
}

pid_alive() {
  [[ -f "$pid_file" ]] || return 1
  local pid
  pid="$(<"$pid_file")"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 1
  kill -0 "$pid" >/dev/null 2>&1
}

print_status() {
  local pid="none"
  if [[ -f "$pid_file" ]]; then
    pid="$(<"$pid_file")"
  fi
  if socket_ready; then
    echo "openrgb_sdk_server=running host=$host port=$port pid=$pid"
  elif pid_alive; then
    echo "openrgb_sdk_server=starting host=$host port=$port pid=$pid"
  else
    echo "openrgb_sdk_server=stopped host=$host port=$port pid=$pid"
  fi
  echo "pid_file=$pid_file"
  echo "stdout_log=$stdout_log"
  echo "stderr_log=$stderr_log"
}

start_server() {
  if socket_ready; then
    print_status
    return 0
  fi
  if pid_alive; then
    print_status
    return 0
  fi
  if ! command -v "$openrgb_bin" >/dev/null 2>&1; then
    echo "openrgb binary not found: $openrgb_bin" >&2
    exit 1
  fi
  openrgb_path="$(command -v "$openrgb_bin")"
  "$openrgb_path" --server --server-host "$host" --server-port "$port" --localconfig --noautoconnect \
    >"$stdout_log" 2>"$stderr_log" &
  echo "$!" >"$pid_file"
  for _ in {1..80}; do
    if socket_ready; then
      sleep "$stability_wait"
      if pid_alive && socket_ready; then
        print_status
        return 0
      fi
      echo "openrgb SDK server exited before the stability window completed" >&2
      print_status
      exit 1
    fi
    if ! pid_alive; then
      echo "openrgb SDK server exited before port became ready" >&2
      print_status
      exit 1
    fi
    sleep 0.1
  done
  echo "openrgb SDK server did not become ready on $host:$port" >&2
  print_status
  exit 1
}

stop_server() {
  if pid_alive; then
    pid="$(<"$pid_file")"
    kill "$pid" >/dev/null 2>&1 || true
    for _ in {1..40}; do
      if ! kill -0 "$pid" >/dev/null 2>&1; then
        break
      fi
      sleep 0.05
    done
  fi
  rm -f "$pid_file"
  print_status
}

case "$action" in
  status)
    print_status
    ;;
  start)
    start_server
    ;;
  stop)
    stop_server
    ;;
  restart)
    stop_server >/dev/null
    start_server
    ;;
esac
