#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/capture-keyboard-rgb-openrgb-sdk-write-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

pick_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

start_fake_server() {
  local port="$1"
  local updates_log="$2"
  python3 - "$port" "$updates_log" >"$tmp/fake-sdk-write-server-$port.stdout" 2>"$tmp/fake-sdk-write-server-$port.stderr" <<'PY' &
import json
import pathlib
import socket
import struct
import sys

port = int(sys.argv[1])
updates_path = pathlib.Path(sys.argv[2])

REQUEST_CONTROLLER_COUNT = 0
REQUEST_CONTROLLER_DATA = 1
REQUEST_PROTOCOL_VERSION = 40
RGBCONTROLLER_UPDATELEDS = 1050
RGBCONTROLLER_UPDATEMODE = 1101
PROTOCOL_VERSION = 3
mode = "Direct"
colors = ["000000", "000000", "000000", "000000"]
updates = []


def pack_string(value):
    raw = value.encode("utf-8") + b"\x00"
    return struct.pack("<H", len(raw)) + raw


def rgb(value):
    return bytes.fromhex(value) + b"\x00"


def mode_payload(name, index, mode_colors):
    out = pack_string(name)
    out += struct.pack("<iIIIII", index, 0, 0, 100, 0, 100)
    out += struct.pack("<IIIIII", 0, len(mode_colors), 50, 50, 0, 0)
    out += struct.pack("<H", len(mode_colors))
    out += b"".join(rgb(color) for color in mode_colors)
    return out


def zone(name):
    return pack_string(name) + struct.pack("<iIIIH", 1, 4, 4, 4, 0)


def led(name):
    return pack_string(name) + struct.pack("<I", 0)


def controller_payload():
    modes = [mode_payload("Direct", 0, colors), mode_payload("Breathing", 1, ["FF0000"])]
    leds = [led("Left side"), led("Left center"), led("Right center"), led("Right side")]
    body = struct.pack("<i", 3)
    body += pack_string("Lenovo 5 2023")
    body += pack_string("Lenovo")
    body += pack_string("Lenovo 4-Zone device")
    body += pack_string("fixture-version")
    body += pack_string("")
    body += pack_string("")
    body += struct.pack("<Hi", len(modes), 1 if mode == "Breathing" else 0)
    body += b"".join(modes)
    body += struct.pack("<H", 1) + zone("Keyboard")
    body += struct.pack("<H", len(leds)) + b"".join(leds)
    body += struct.pack("<H", len(colors)) + b"".join(rgb(color) for color in colors)
    return struct.pack("<I", len(body) + 4) + body


def recv_exact(conn, size):
    data = b""
    while len(data) < size:
        chunk = conn.recv(size - len(data))
        if not chunk:
            raise SystemExit(0)
        data += chunk
    return data


def send(conn, dev_idx, packet_id, payload):
    conn.sendall(b"ORGB" + struct.pack("<III", dev_idx, packet_id, len(payload)) + payload)


def parse_update_leds(payload):
    data_size = struct.unpack("<I", payload[:4])[0]
    count = struct.unpack("<H", payload[4:6])[0]
    out = []
    offset = 6
    for _ in range(count):
        r, g, b, _pad = payload[offset:offset + 4]
        out.append(f"{r:02X}{g:02X}{b:02X}")
        offset += 4
    if data_size != len(payload):
        raise SystemExit(f"bad data_size {data_size} for payload {len(payload)}")
    return out


def parse_update_mode(payload):
    data_size = struct.unpack("<I", payload[:4])[0]
    mode_idx = struct.unpack("<i", payload[4:8])[0]
    if data_size != len(payload):
        raise SystemExit(f"bad mode data_size {data_size} for payload {len(payload)}")
    if mode_idx == 0:
        return "Direct"
    if mode_idx == 1:
        return "Breathing"
    raise SystemExit(f"unexpected mode index {mode_idx}")


with socket.socket() as server:
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(1)
    conn, _ = server.accept()
    with conn:
        while True:
            header = recv_exact(conn, 16)
            magic, dev_idx, packet_id, size = struct.unpack("<4sIII", header)
            if magic != b"ORGB":
                raise SystemExit(2)
            payload = recv_exact(conn, size)
            if packet_id == REQUEST_PROTOCOL_VERSION:
                send(conn, 0, packet_id, struct.pack("<I", PROTOCOL_VERSION))
            elif packet_id == REQUEST_CONTROLLER_COUNT:
                send(conn, 0, packet_id, struct.pack("<I", 1))
            elif packet_id == REQUEST_CONTROLLER_DATA:
                send(conn, dev_idx, packet_id, controller_payload())
            elif packet_id == RGBCONTROLLER_UPDATELEDS:
                colors = parse_update_leds(payload)
                updates.append(colors)
                updates_path.write_text(json.dumps(updates) + "\n")
            elif packet_id == RGBCONTROLLER_UPDATEMODE:
                mode = parse_update_mode(payload)
                updates.append({"mode": mode})
                updates_path.write_text(json.dumps(updates) + "\n")
            else:
                raise SystemExit(f"unexpected packet {packet_id}")
PY
}

dry_port="$(pick_port)"
dry_updates="$tmp/dry-updates.json"
start_fake_server "$dry_port" "$dry_updates"
dry_pid=$!
"$script" --output "$tmp/dry" --no-start-server --host 127.0.0.1 --port "$dry_port" >/dev/null
kill "$dry_pid" >/dev/null 2>&1 || true
wait "$dry_pid" >/dev/null 2>&1 || true

python3 - "$tmp/dry/openrgb-keyboard-rgb-sdk-write-evidence.json" "$dry_updates" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
updates_path = pathlib.Path(sys.argv[2])
if report["result"]["status"] != "dry_run":
    raise SystemExit(report["result"])
if report["commands"]["write_sent"]:
    raise SystemExit("dry-run must not send write")
if updates_path.exists():
    raise SystemExit(f"dry-run sent updates: {updates_path.read_text()}")
PY

execute_port="$(pick_port)"
execute_updates="$tmp/execute-updates.json"
start_fake_server "$execute_port" "$execute_updates"
execute_pid=$!
"$script" --output "$tmp/execute" --no-start-server --host 127.0.0.1 --port "$execute_port" --execute >/dev/null
kill "$execute_pid" >/dev/null 2>&1 || true
wait "$execute_pid" >/dev/null 2>&1 || true

python3 - "$tmp/execute/openrgb-keyboard-rgb-sdk-write-evidence.json" "$execute_updates" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
updates = json.loads(pathlib.Path(sys.argv[2]).read_text())
if updates != [
    {"mode": "Breathing"},
    ["FF0000", "00FF00", "0000FF", "FFFFFF"],
    {"mode": "Direct"},
    ["000000", "000000", "000000", "000000"],
]:
    raise SystemExit(f"unexpected SDK writes: {updates}")
if not report["result"]["sdk_write_ready_evidence"]:
    raise SystemExit(report["result"])
readback = report["readback"]
if not readback["mode_readback_matches"] or not readback["color_readback_matches"] or not readback["restore_color_matches"]:
    raise SystemExit(readback)
if not report["commands"]["mode_write_sent"] or not report["commands"]["write_sent"] or not report["commands"]["mode_restore_sent"] or not report["commands"]["restore_sent"]:
    raise SystemExit(report["commands"])
PY

echo "capture-keyboard-rgb-openrgb-sdk-write-evidence tests passed"
