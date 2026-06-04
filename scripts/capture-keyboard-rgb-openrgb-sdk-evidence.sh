#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh --output <dir> [options]

Capture read-only OpenRGB SDK controller evidence for keyboard RGB promotion.

This script starts a temporary local OpenRGB SDK server by default, enumerates
controllers through the SDK protocol, and records the detected Lenovo keyboard
controller's active mode, modes, zones, LEDs, and colors. It does not set modes,
colors, profiles, hidraw, i2c, WMI, EC, or sysfs.

Options:
  --output <dir>          Required output directory.
  --openrgb-bin <path>    OpenRGB binary to run. Default: openrgb from PATH.
  --host <host>           SDK host. Default: 127.0.0.1.
  --port <port>           SDK port. Default: random free port when starting server.
  --no-start-server       Connect to an already-running SDK server.
  -h, --help              Show this help.
EOF
}

output=""
openrgb_bin="openrgb"
host="127.0.0.1"
port=""
start_server=1

while (($#)); do
  case "$1" in
    --output)
      output="${2:?missing value for --output}"
      shift 2
      ;;
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
    --no-start-server)
      start_server=0
      shift
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

if [[ -z "$output" ]]; then
  echo "--output is required" >&2
  usage >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate OpenRGB SDK evidence" >&2
  exit 1
}

mkdir -p "$output/logs"
output="$(cd "$output" && pwd)"

json_out="$output/openrgb-keyboard-rgb-sdk-evidence.json"
md_out="$output/openrgb-keyboard-rgb-sdk-evidence.md"
server_stdout="$output/logs/openrgb-sdk-server.stdout"
server_stderr="$output/logs/openrgb-sdk-server.stderr"

if [[ "$start_server" -eq 1 ]]; then
  if ! command -v "$openrgb_bin" >/dev/null 2>&1; then
    python3 - "$json_out" "$md_out" "$openrgb_bin" <<'PY'
import datetime as dt
import json
import pathlib
import sys

json_path = pathlib.Path(sys.argv[1])
md_path = pathlib.Path(sys.argv[2])
openrgb_bin = sys.argv[3]
report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "openrgb": {"installed": False, "path": None, "requested_binary": openrgb_bin},
    "sdk": {"connected": False, "server_started": False},
    "keyboard": {"detected": False},
    "result": {
        "status": "openrgb_missing",
        "read_back_supported": False,
        "promotion_blockers": ["openrgb binary was not found"],
    },
}
json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
md_path.write_text("# OpenRGB SDK Evidence\n\n- status: `openrgb_missing`\n")
PY
    echo "openrgb_sdk_evidence=$json_out"
    exit 0
  fi
  openrgb_path="$(command -v "$openrgb_bin")"
  if [[ -z "$port" ]]; then
    port="$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"
  fi
  "$openrgb_path" --server --server-host "$host" --server-port "$port" --localconfig --noautoconnect \
    >"$server_stdout" 2>"$server_stderr" &
  server_pid=$!
  trap 'kill "$server_pid" >/dev/null 2>&1 || true; wait "$server_pid" >/dev/null 2>&1 || true' EXIT
else
  openrgb_path="$(command -v "$openrgb_bin" 2>/dev/null || true)"
  : >"$server_stdout"
  : >"$server_stderr"
  if [[ -z "$port" ]]; then
    echo "--port is required with --no-start-server" >&2
    exit 2
  fi
fi

python3 - "$json_out" "$md_out" "$host" "$port" "$start_server" "${openrgb_path:-}" <<'PY'
import datetime as dt
import json
import pathlib
import socket
import struct
import sys
import time

(
    json_path,
    md_path,
    host,
    port,
    server_started,
    openrgb_path,
) = sys.argv[1:]

json_path = pathlib.Path(json_path)
md_path = pathlib.Path(md_path)
port_i = int(port)
server_started_b = bool(int(server_started))
CLIENT_PROTOCOL_VERSION = 4

REQUEST_CONTROLLER_COUNT = 0
REQUEST_CONTROLLER_DATA = 1
REQUEST_PROTOCOL_VERSION = 40
DEVICE_LIST_UPDATED = 100


class Reader:
    def __init__(self, data):
        self.data = data
        self.offset = 0

    def take(self, size):
        if self.offset + size > len(self.data):
            raise ValueError(f"wanted {size} bytes at {self.offset}, only {len(self.data)} available")
        out = self.data[self.offset : self.offset + size]
        self.offset += size
        return out

    def u16(self):
        return struct.unpack("<H", self.take(2))[0]

    def u32(self):
        return struct.unpack("<I", self.take(4))[0]

    def i32(self):
        return struct.unpack("<i", self.take(4))[0]

    def string(self):
        size = self.u16()
        raw = self.take(size)
        if raw.endswith(b"\x00"):
            raw = raw[:-1]
        return raw.decode("utf-8", errors="replace")


def recv_exact(sock, size):
    data = b""
    while len(data) < size:
        chunk = sock.recv(size - len(data))
        if not chunk:
            raise ConnectionError("SDK server closed connection")
        data += chunk
    return data


def send_packet(sock, dev_idx, packet_id, payload=b""):
    sock.sendall(b"ORGB" + struct.pack("<III", dev_idx, packet_id, len(payload)) + payload)


def recv_packet(sock):
    header = recv_exact(sock, 16)
    magic, dev_idx, packet_id, size = struct.unpack("<4sIII", header)
    if magic != b"ORGB":
        raise ValueError(f"unexpected SDK packet magic {magic!r}")
    return dev_idx, packet_id, recv_exact(sock, size)


def recv_expected(sock, expected_packet_id, expected_dev_idx=None):
    skipped = []
    deadline = time.time() + 5
    while True:
        dev_idx, packet_id, payload = recv_packet(sock)
        if packet_id == expected_packet_id and (
            expected_dev_idx is None or dev_idx == expected_dev_idx
        ):
            return dev_idx, packet_id, payload, skipped
        skipped.append({
            "device_index": dev_idx,
            "packet_id": packet_id,
            "payload_bytes": len(payload),
        })
        if time.time() >= deadline:
            raise ValueError(
                f"expected SDK packet {expected_packet_id}, saw {skipped[-4:]}"
            )


def parse_rgb(reader):
    r = reader.take(1)[0]
    g = reader.take(1)[0]
    b = reader.take(1)[0]
    reader.take(1)
    return f"#{r:02X}{g:02X}{b:02X}"


def parse_mode(reader, version):
    mode = {
        "name": reader.string(),
        "value": reader.i32(),
        "flags": reader.u32(),
        "speed_min": reader.u32(),
        "speed_max": reader.u32(),
    }
    if version >= 3:
        mode["brightness_min"] = reader.u32()
        mode["brightness_max"] = reader.u32()
    mode["colors_min"] = reader.u32()
    mode["colors_max"] = reader.u32()
    mode["speed"] = reader.u32()
    if version >= 3:
        mode["brightness"] = reader.u32()
    mode["direction"] = reader.u32()
    mode["color_mode"] = reader.u32()
    colors = []
    for _ in range(reader.u16()):
        colors.append(parse_rgb(reader))
    mode["colors"] = colors
    return mode


def parse_zone(reader, version):
    zone = {
        "name": reader.string(),
        "type": reader.i32(),
        "leds_min": reader.u32(),
        "leds_max": reader.u32(),
        "num_leds": reader.u32(),
    }
    matrix_size = reader.u16()
    zone["matrix_size"] = matrix_size
    if matrix_size:
        zone["matrix_height"] = reader.u32()
        zone["matrix_width"] = reader.u32()
        if matrix_size < 8:
            raise ValueError(f"invalid zone matrix size {matrix_size}")
        reader.take(matrix_size - 8)
    if version >= 4:
        segment_count = reader.u16()
        zone["segment_count"] = segment_count
        segments = []
        for _ in range(segment_count):
            segments.append({
                "name": reader.string(),
                "type": reader.i32(),
                "start_index": reader.u32(),
                "led_count": reader.u32(),
            })
        zone["segments"] = segments
    if version >= 5:
        zone["flags"] = reader.u32()
    return zone


def parse_led(reader, _version):
    return {"name": reader.string(), "value": reader.u32()}


def parse_list(reader, parser, version):
    count = reader.u16()
    return [parser(reader, version) for _ in range(count)]


def parse_controller(payload, version, index):
    reader = Reader(payload)
    data_size = reader.u32()
    controller = {
        "index": index,
        "data_size": data_size,
        "device_type": reader.i32(),
        "name": reader.string(),
        "vendor": reader.string(),
    }
    if version >= 1:
        controller["description"] = reader.string()
    controller["version"] = reader.string()
    controller["serial"] = reader.string()
    controller["location"] = reader.string()
    mode_count = reader.u16()
    active_mode = reader.i32()
    modes = [parse_mode(reader, version) for _ in range(mode_count)]
    controller["modes"] = modes
    controller["active_mode_index"] = active_mode
    controller["active_mode"] = (
        modes[active_mode]["name"] if 0 <= active_mode < len(modes) else None
    )
    controller["zones"] = parse_list(reader, parse_zone, version)
    controller["leds"] = parse_list(reader, parse_led, version)
    colors = parse_list(reader, lambda r, v: parse_rgb(r), version)
    controller["colors"] = colors
    if version >= 5:
        controller["led_alt_names"] = [reader.string() for _ in range(reader.u16())]
        controller["flags"] = reader.u32()
    controller["parse_complete"] = reader.offset == len(payload)
    controller["unparsed_bytes"] = len(payload) - reader.offset
    return controller


def connect_with_retry():
    deadline = time.time() + 10
    last_error = None
    while time.time() < deadline:
        try:
            sock = socket.create_connection((host, port_i), timeout=2)
            sock.settimeout(5)
            return sock
        except OSError as error:
            last_error = error
            time.sleep(0.25)
    raise ConnectionError(f"could not connect to OpenRGB SDK server: {last_error}")


controllers = []
promotion_blockers = []
status = "ok"
connected = False
protocol_version = None
controller_count = None
controller_count_attempts = 0
skipped_packets = []
keyboard = None

try:
    with connect_with_retry() as sock:
        connected = True
        send_packet(sock, 0, REQUEST_PROTOCOL_VERSION, struct.pack("<I", CLIENT_PROTOCOL_VERSION))
        _, _, payload, skipped = recv_expected(sock, REQUEST_PROTOCOL_VERSION)
        skipped_packets.extend(skipped)
        protocol_version = struct.unpack("<I", payload[:4])[0] if len(payload) >= 4 else 0
        negotiated = min(protocol_version, CLIENT_PROTOCOL_VERSION)

        deadline = time.time() + (12 if server_started_b else 2)
        while True:
            controller_count_attempts += 1
            send_packet(sock, 0, REQUEST_CONTROLLER_COUNT)
            _, _, payload, skipped = recv_expected(sock, REQUEST_CONTROLLER_COUNT)
            skipped_packets.extend(skipped)
            controller_count = struct.unpack("<I", payload[:4])[0] if len(payload) >= 4 else 0
            if controller_count > 0 or time.time() >= deadline:
                break
            time.sleep(0.5)

        for index in range(controller_count):
            request_payload = struct.pack("<I", negotiated) if negotiated >= 1 else b""
            send_packet(sock, index, REQUEST_CONTROLLER_DATA, request_payload)
            _, _, payload, skipped = recv_expected(
                sock, REQUEST_CONTROLLER_DATA, expected_dev_idx=index
            )
            skipped_packets.extend(skipped)
            try:
                controllers.append(parse_controller(payload, negotiated, index))
            except Exception as error:
                controllers.append({
                    "index": index,
                    "parse_error": str(error),
                    "payload_bytes": len(payload),
                })
                promotion_blockers.append(f"controller {index} SDK data parse failed: {error}")
except Exception as error:
    status = "sdk_unavailable"
    promotion_blockers.append(str(error))

for controller in controllers:
    haystack = " ".join(
        str(controller.get(key, ""))
        for key in ("name", "vendor", "description")
    ).lower()
    zones = " ".join(zone.get("name", "") for zone in controller.get("zones", []))
    if "lenovo" in haystack and ("keyboard" in haystack or "4-zone" in haystack or "keyboard" in zones.lower()):
        keyboard = controller
        break

if status == "ok" and keyboard is None:
    status = "keyboard_not_found"
    if not controllers:
        promotion_blockers.append("OpenRGB SDK reported zero controllers after detection wait")
    promotion_blockers.append("OpenRGB SDK did not report a Lenovo keyboard controller")

read_back_supported = bool(
    status == "ok"
    and keyboard
    and keyboard.get("active_mode") is not None
    and keyboard.get("colors")
)
if status == "ok" and not read_back_supported:
    promotion_blockers.append("SDK controller data did not include both active mode and colors")

report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "openrgb": {
        "installed": bool(openrgb_path),
        "path": openrgb_path or None,
    },
    "sdk": {
        "host": host,
        "port": port_i,
        "connected": connected,
        "server_started": server_started_b,
        "protocol_version": protocol_version,
        "client_protocol_version": CLIENT_PROTOCOL_VERSION,
        "controller_count": controller_count,
        "controller_count_attempts": controller_count_attempts,
        "skipped_packets": skipped_packets,
    },
    "controllers": controllers,
    "keyboard": {
        "detected": keyboard is not None,
        "controller": keyboard,
    },
    "result": {
        "status": status,
        "read_back_supported": read_back_supported,
        "promotion_blockers": promotion_blockers,
    },
    "safety": {
        "read_only": True,
        "no_rgb_writes": True,
        "no_hidraw_writes": True,
        "no_i2c_writes": True,
    },
}
json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

lines = [
    "# OpenRGB SDK Evidence",
    "",
    f"- status: `{status}`",
    f"- connected: `{connected}`",
    f"- protocol_version: `{protocol_version}`",
    f"- controller_count: `{controller_count}`",
    f"- controller_count_attempts: `{controller_count_attempts}`",
    f"- keyboard_detected: `{keyboard is not None}`",
    f"- read_back_supported: `{read_back_supported}`",
]
if keyboard:
    lines.extend([
        f"- device: `{keyboard.get('name')}`",
        f"- active_mode: `{keyboard.get('active_mode')}`",
        f"- led_count: `{len(keyboard.get('leds', []))}`",
        f"- color_count: `{len(keyboard.get('colors', []))}`",
    ])
lines.append("")
lines.append("## Promotion Blockers")
lines.extend(f"- {blocker}" for blocker in promotion_blockers)
lines.append("")
md_path.write_text("\n".join(lines))
PY

echo "openrgb_sdk_evidence=$json_out"
