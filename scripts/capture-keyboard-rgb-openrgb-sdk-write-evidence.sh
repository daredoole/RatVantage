#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-keyboard-rgb-openrgb-sdk-write-evidence.sh --output <dir> [options]

Capture operator-triggered OpenRGB SDK write evidence for keyboard RGB promotion.

Default mode is dry-run: it starts/connects to the OpenRGB SDK server, reads the
Lenovo keyboard controller, and records the SDK packets that would be sent. It
does not write mode/colors unless --execute is supplied.

Options:
  --output <dir>          Required output directory.
  --openrgb-bin <path>    OpenRGB binary to run. Default: openrgb from PATH.
  --host <host>           SDK host. Default: 127.0.0.1.
  --port <port>           SDK port. Default: random free port when starting server.
  --mode <name>           Mode to write before colors. Default: Breathing.
  --colors <csv>          LED-order colors without '#'. Default: FF0000,00FF00,0000FF,FFFFFF.
  --no-start-server       Connect to an already-running SDK server.
  --execute               Send UPDATEMODE/UPDATELEDS, read back, then restore before mode/colors.
  -h, --help              Show this help.

Execute mode changes keyboard RGB briefly. It writes only through OpenRGB's SDK
server, never through hidraw, i2c, sysfs, WMI, or EC directly.
EOF
}

output=""
openrgb_bin="openrgb"
host="127.0.0.1"
port=""
mode="Breathing"
colors="FF0000,00FF00,0000FF,FFFFFF"
start_server=1
execute=0

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
    --mode)
      mode="${2:?missing value for --mode}"
      shift 2
      ;;
    --colors)
      colors="${2:?missing value for --colors}"
      shift 2
      ;;
    --no-start-server)
      start_server=0
      shift
      ;;
    --execute)
      execute=1
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
if [[ ! "$colors" =~ ^[0-9A-Fa-f]{6}(,[0-9A-Fa-f]{6})*$ ]]; then
  echo "--colors must be comma-separated RRGGBB hex values" >&2
  exit 2
fi
command -v python3 >/dev/null 2>&1 || {
  echo "missing python3; install Python 3 to generate OpenRGB SDK write evidence" >&2
  exit 1
}

mkdir -p "$output/logs"
output="$(cd "$output" && pwd)"

json_out="$output/openrgb-keyboard-rgb-sdk-write-evidence.json"
md_out="$output/openrgb-keyboard-rgb-sdk-write-evidence.md"
server_stdout="$output/logs/openrgb-sdk-write-server.stdout"
server_stderr="$output/logs/openrgb-sdk-write-server.stderr"

if [[ "$start_server" -eq 1 ]]; then
  if ! command -v "$openrgb_bin" >/dev/null 2>&1; then
    python3 - "$json_out" "$md_out" "$openrgb_bin" "$execute" "$colors" "$mode" <<'PY'
import datetime as dt
import json
import pathlib
import sys

json_path = pathlib.Path(sys.argv[1])
md_path = pathlib.Path(sys.argv[2])
openrgb_bin = sys.argv[3]
execute = bool(int(sys.argv[4]))
colors = sys.argv[5]
mode = sys.argv[6]
report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "execute": execute,
    "openrgb": {"installed": False, "path": None, "requested_binary": openrgb_bin},
    "request": {"mode": mode, "colors": colors.upper(), "packets": ["RGBCONTROLLER_UPDATEMODE", "RGBCONTROLLER_UPDATELEDS"]},
    "result": {
        "status": "openrgb_missing",
        "sdk_write_ready_evidence": False,
        "promotion_blockers": ["openrgb binary was not found"],
    },
}
json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
md_path.write_text("# OpenRGB SDK Write Evidence\n\n- status: `openrgb_missing`\n")
PY
    echo "openrgb_sdk_write_evidence=$json_out"
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

python3 - "$json_out" "$md_out" "$host" "$port" "$start_server" "${openrgb_path:-}" "$execute" "$colors" "$mode" <<'PY'
import datetime as dt
import json
import pathlib
import re
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
    execute,
    requested_colors_raw,
    requested_mode,
) = sys.argv[1:]

json_path = pathlib.Path(json_path)
md_path = pathlib.Path(md_path)
port_i = int(port)
server_started_b = bool(int(server_started))
execute_b = bool(int(execute))
requested_colors = [value.upper() for value in requested_colors_raw.split(",") if value]
CLIENT_PROTOCOL_VERSION = 4

REQUEST_CONTROLLER_COUNT = 0
REQUEST_CONTROLLER_DATA = 1
REQUEST_PROTOCOL_VERSION = 40
DEVICE_LIST_UPDATED = 100
RGBCONTROLLER_UPDATELEDS = 1050
RGBCONTROLLER_UPDATEMODE = 1101


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
            raise ValueError(f"expected SDK packet {expected_packet_id}, saw {skipped[-4:]}")


def parse_rgb(reader):
    r = reader.take(1)[0]
    g = reader.take(1)[0]
    b = reader.take(1)[0]
    reader.take(1)
    return f"{r:02X}{g:02X}{b:02X}"


def pack_string(value):
    raw = value.encode("utf-8") + b"\x00"
    return struct.pack("<H", len(raw)) + raw


def pack_rgb(color):
    if not re.fullmatch(r"[0-9A-F]{6}", color):
        raise ValueError(f"invalid color {color!r}")
    return bytes.fromhex(color) + b"\x00"


def rgb_payload(colors):
    payload = bytearray()
    for color in colors:
        payload.extend(pack_rgb(color))
    body = struct.pack("<H", len(colors)) + bytes(payload)
    return struct.pack("<I", len(body) + 4) + body


def mode_data_payload(mode, version, colors=None):
    mode_colors = colors if colors is not None else mode.get("colors") or []
    out = bytearray()
    out.extend(pack_string(mode["name"]))
    out.extend(struct.pack("<i", int(mode["value"])))
    out.extend(struct.pack("<I", int(mode.get("flags") or 0)))
    out.extend(struct.pack("<I", int(mode.get("speed_min") or 0)))
    out.extend(struct.pack("<I", int(mode.get("speed_max") or 0)))
    if version >= 3:
        out.extend(struct.pack("<I", int(mode.get("brightness_min") or 0)))
        out.extend(struct.pack("<I", int(mode.get("brightness_max") or 0)))
    out.extend(struct.pack("<I", int(mode.get("colors_min") or 0)))
    out.extend(struct.pack("<I", int(mode.get("colors_max") or 0)))
    out.extend(struct.pack("<I", int(mode.get("speed") or 0)))
    if version >= 3:
        out.extend(struct.pack("<I", int(mode.get("brightness") or 0)))
    out.extend(struct.pack("<I", int(mode.get("direction") or 0)))
    out.extend(struct.pack("<I", int(mode.get("color_mode") or 0)))
    out.extend(struct.pack("<H", len(mode_colors)))
    for color in mode_colors:
        out.extend(pack_rgb(color))
    return bytes(out)


def update_mode_payload(mode, version, colors=None):
    mode_data = mode_data_payload(mode, version, colors)
    body = struct.pack("<i", int(mode["value"])) + mode_data
    return struct.pack("<I", len(body) + 4) + body


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
    mode["colors"] = [parse_rgb(reader) for _ in range(reader.u16())]
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
        if matrix_size < 8:
            raise ValueError(f"invalid zone matrix size {matrix_size}")
        zone["matrix_height"] = reader.u32()
        zone["matrix_width"] = reader.u32()
        reader.take(matrix_size - 8)
    if version >= 4:
        zone["segments"] = [
            {
                "name": reader.string(),
                "type": reader.i32(),
                "start_index": reader.u32(),
                "led_count": reader.u32(),
            }
            for _ in range(reader.u16())
        ]
    return zone


def parse_controller(payload, version, index):
    reader = Reader(payload)
    controller = {
        "index": index,
        "data_size": reader.u32(),
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
    controller["active_mode"] = modes[active_mode]["name"] if 0 <= active_mode < len(modes) else None
    controller["zones"] = [parse_zone(reader, version) for _ in range(reader.u16())]
    controller["leds"] = [{"name": reader.string(), "value": reader.u32()} for _ in range(reader.u16())]
    controller["colors"] = [parse_rgb(reader) for _ in range(reader.u16())]
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


def find_keyboard(controllers):
    for controller in controllers:
        haystack = " ".join(
            str(controller.get(key, "")) for key in ("name", "vendor", "description")
        ).lower()
        zones = " ".join(zone.get("name", "") for zone in controller.get("zones", [])).lower()
        if "lenovo" in haystack and ("keyboard" in haystack or "4-zone" in haystack or "keyboard" in zones):
            return controller
    return None


def find_mode(controller, name):
    for mode in controller.get("modes") or []:
        if str(mode.get("name", "")).lower() == name.lower():
            return mode
    return None


def snapshot(controller):
    if not controller:
        return {"detected": False}
    return {
        "detected": True,
        "index": controller.get("index"),
        "name": controller.get("name"),
        "description": controller.get("description"),
        "active_mode": controller.get("active_mode"),
        "colors": controller.get("colors") or [],
        "led_count": len(controller.get("leds") or []),
        "color_count": len(controller.get("colors") or []),
    }


controllers = []
promotion_blockers = []
status = "ok"
connected = False
protocol_version = None
controller_count = None
controller_count_attempts = 0
skipped_packets = []
before = after = restored = None
keyboard = None
mode_write_sent = False
write_sent = False
mode_restore_sent = False
restore_sent = False

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

        def read_controllers():
            items = []
            for index in range(controller_count or 0):
                payload_req = struct.pack("<I", negotiated) if negotiated >= 1 else b""
                send_packet(sock, index, REQUEST_CONTROLLER_DATA, payload_req)
                _, _, data, skipped = recv_expected(sock, REQUEST_CONTROLLER_DATA, expected_dev_idx=index)
                skipped_packets.extend(skipped)
                items.append(parse_controller(data, negotiated, index))
            return items

        controllers = read_controllers()
        keyboard = find_keyboard(controllers)
        before = snapshot(keyboard)

        if execute_b and keyboard:
            index = keyboard["index"]
            before_colors = before.get("colors") or []
            requested_mode_obj = find_mode(keyboard, requested_mode)
            before_mode_obj = find_mode(keyboard, before.get("active_mode") or "")
            if not requested_mode_obj:
                promotion_blockers.append(f"requested mode {requested_mode!r} was not advertised by the SDK controller")
            if not before_mode_obj:
                promotion_blockers.append("before active mode object was not found in SDK controller modes")
            if len(requested_colors) != len(before_colors):
                promotion_blockers.append(
                    f"requested color count {len(requested_colors)} does not match SDK color count {len(before_colors)}"
                )
            if not promotion_blockers:
                send_packet(sock, index, RGBCONTROLLER_UPDATEMODE, update_mode_payload(requested_mode_obj, negotiated, requested_colors[: len(requested_mode_obj.get("colors") or [])] or None))
                mode_write_sent = True
                time.sleep(0.35)
                send_packet(sock, index, RGBCONTROLLER_UPDATELEDS, rgb_payload(requested_colors))
                write_sent = True
                time.sleep(0.35)
                after = snapshot(find_keyboard(read_controllers()))
                send_packet(sock, index, RGBCONTROLLER_UPDATEMODE, update_mode_payload(before_mode_obj, negotiated))
                mode_restore_sent = True
                time.sleep(0.35)
                send_packet(sock, index, RGBCONTROLLER_UPDATELEDS, rgb_payload(before_colors))
                restore_sent = True
                time.sleep(0.35)
                restored = snapshot(find_keyboard(read_controllers()))
except Exception as error:
    status = "sdk_unavailable"
    promotion_blockers.append(str(error))

if status == "ok" and keyboard is None:
    status = "keyboard_not_found"
    if not controllers:
        promotion_blockers.append("OpenRGB SDK reported zero controllers after detection wait")
    promotion_blockers.append("OpenRGB SDK did not report a Lenovo keyboard controller")

if execute_b and not mode_write_sent and status == "ok":
    promotion_blockers.append("SDK mode write was not sent")
if execute_b and mode_write_sent and not write_sent:
    promotion_blockers.append("SDK color write was not sent")
if execute_b and not write_sent and status == "ok":
    promotion_blockers.append("SDK write was not sent")
if execute_b and write_sent and not mode_restore_sent:
    promotion_blockers.append("SDK mode restore was not sent")
if execute_b and mode_restore_sent and not restore_sent:
    promotion_blockers.append("SDK restore was not sent")

if not execute_b:
    promotion_blockers.append("dry-run only; execute mode has not captured SDK write/read-back/restore evidence")

after = after or {"detected": False}
restored = restored or {"detected": False}
before_colors = before.get("colors") if before else []
after_colors = after.get("colors") or []
restored_colors = restored.get("colors") or []
mode_readback_matches = bool(execute_b and after.get("active_mode") and after.get("active_mode").lower() == requested_mode.lower())
color_readback_matches = bool(execute_b and after_colors[: len(requested_colors)] == requested_colors)
restore_color_matches = bool(execute_b and restored_colors == before_colors and before_colors)
restore_mode_matches = bool(execute_b and restored.get("active_mode") == before.get("active_mode") and before.get("active_mode")) if before else False
sdk_write_ready_evidence = bool(
    status == "ok"
    and execute_b
    and mode_write_sent
    and write_sent
    and mode_restore_sent
    and restore_sent
    and mode_readback_matches
    and color_readback_matches
    and restore_color_matches
    and restore_mode_matches
    and not promotion_blockers
)

if execute_b and not mode_readback_matches:
    promotion_blockers.append("SDK active mode read-back did not match requested mode after write")
if execute_b and not color_readback_matches:
    promotion_blockers.append("SDK color read-back did not match requested colors after write")
if execute_b and not restore_color_matches:
    promotion_blockers.append("SDK color read-back did not return to before colors after restore")
if execute_b and not restore_mode_matches:
    promotion_blockers.append("SDK active mode did not return to before mode after restore")

report = {
    "schema_version": 1,
    "generated_at_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat(),
    "execute": execute_b,
    "openrgb": {"installed": bool(openrgb_path), "path": openrgb_path or None},
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
    "request": {
        "mode": requested_mode,
        "packets": ["RGBCONTROLLER_UPDATEMODE", "RGBCONTROLLER_UPDATELEDS"],
        "packet_ids": [RGBCONTROLLER_UPDATEMODE, RGBCONTROLLER_UPDATELEDS],
        "colors": requested_colors,
    },
    "readback": {
        "before": before,
        "after": after if execute_b else None,
        "restored": restored if execute_b else None,
        "mode_readback_matches": mode_readback_matches,
        "color_readback_matches": color_readback_matches,
        "restore_color_matches": restore_color_matches,
        "restore_mode_matches": restore_mode_matches,
    },
    "commands": {
        "mode_write_sent": mode_write_sent,
        "write_sent": write_sent,
        "mode_restore_sent": mode_restore_sent,
        "restore_sent": restore_sent,
    },
    "result": {
        "status": "executed" if execute_b else "dry_run",
        "sdk_write_ready_evidence": sdk_write_ready_evidence,
        "promotion_blockers": promotion_blockers,
    },
    "safety": {
        "operator_triggered_write": execute_b,
        "no_sysfs_writes": True,
        "no_hidraw_writes": True,
        "no_i2c_writes": True,
        "no_wmi_writes": True,
        "no_ec_writes": True,
    },
}
json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

lines = [
    "# OpenRGB SDK Write Evidence",
    "",
    f"- status: `{report['result']['status']}`",
    f"- connected: `{connected}`",
    f"- protocol_version: `{protocol_version}`",
    f"- keyboard_detected: `{bool(before and before.get('detected'))}`",
    f"- requested_mode: `{requested_mode}`",
    f"- before_mode: `{before.get('active_mode') if before else None}`",
    f"- after_mode: `{after.get('active_mode')}`",
    f"- restored_mode: `{restored.get('active_mode')}`",
    f"- after_colors: `{','.join(after_colors)}`",
    f"- restored_colors: `{','.join(restored_colors)}`",
    f"- mode_readback_matches: `{mode_readback_matches}`",
    f"- color_readback_matches: `{color_readback_matches}`",
    f"- restore_color_matches: `{restore_color_matches}`",
    f"- sdk_write_ready_evidence: `{sdk_write_ready_evidence}`",
    "",
    "## Promotion Blockers",
]
lines.extend(f"- {blocker}" for blocker in promotion_blockers)
lines.append("")
md_path.write_text("\n".join(lines))
PY

echo "openrgb_sdk_write_evidence=$json_out"
