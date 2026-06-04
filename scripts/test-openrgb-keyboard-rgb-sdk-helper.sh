#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
helper="$repo_root/scripts/openrgb-keyboard-rgb-sdk-helper.sh"
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

port="$(pick_port)"
state="$tmp/state.json"
calls="$tmp/calls.json"
python3 - "$port" "$state" "$calls" >"$tmp/server.stdout" 2>"$tmp/server.stderr" <<'PY' &
import json
import pathlib
import socket
import struct
import sys

port = int(sys.argv[1])
state_path = pathlib.Path(sys.argv[2])
calls_path = pathlib.Path(sys.argv[3])

REQUEST_CONTROLLER_COUNT = 0
REQUEST_CONTROLLER_DATA = 1
REQUEST_PROTOCOL_VERSION = 40
RGBCONTROLLER_UPDATELEDS = 1050
RGBCONTROLLER_UPDATEMODE = 1101
PROTOCOL_VERSION = 3
state = {"mode": "Direct", "colors": ["000000", "000000", "000000", "000000"]}
calls = []


def save():
    state_path.write_text(json.dumps(state) + "\n")
    calls_path.write_text(json.dumps(calls) + "\n")


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
    modes = [mode_payload("Direct", 0, state["colors"]), mode_payload("Breathing", 1, ["FF0000"])]
    leds = [led("Left side"), led("Left center"), led("Right center"), led("Right side")]
    body = struct.pack("<i", 3)
    body += pack_string("Lenovo 5 2023")
    body += pack_string("Lenovo")
    body += pack_string("Lenovo 4-Zone device")
    body += pack_string("fixture-version")
    body += pack_string("")
    body += pack_string("")
    body += struct.pack("<Hi", len(modes), 1 if state["mode"] == "Breathing" else 0)
    body += b"".join(modes)
    body += struct.pack("<H", 1) + zone("Keyboard")
    body += struct.pack("<H", len(leds)) + b"".join(leds)
    body += struct.pack("<H", len(state["colors"])) + b"".join(rgb(color) for color in state["colors"])
    return struct.pack("<I", len(body) + 4) + body


def recv_exact(conn, size):
    data = b""
    while len(data) < size:
        chunk = conn.recv(size - len(data))
        if not chunk:
            raise EOFError
        data += chunk
    return data


def send(conn, dev_idx, packet_id, payload):
    conn.sendall(b"ORGB" + struct.pack("<III", dev_idx, packet_id, len(payload)) + payload)


def parse_update_leds(payload):
    count = struct.unpack("<H", payload[4:6])[0]
    out = []
    offset = 6
    for _ in range(count):
        r, g, b, _pad = payload[offset:offset + 4]
        out.append(f"{r:02X}{g:02X}{b:02X}")
        offset += 4
    return out


def parse_update_mode(payload):
    mode_idx = struct.unpack("<i", payload[4:8])[0]
    if mode_idx == 0:
        return "Direct"
    if mode_idx == 1:
        return "Breathing"
    raise SystemExit(f"unexpected mode index {mode_idx}")


save()
with socket.socket() as server:
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(8)
    while True:
        conn, _ = server.accept()
        with conn:
            while True:
                try:
                    header = recv_exact(conn, 16)
                except EOFError:
                    break
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
                elif packet_id == RGBCONTROLLER_UPDATEMODE:
                    state["mode"] = parse_update_mode(payload)
                    calls.append({"mode": state["mode"]})
                    save()
                elif packet_id == RGBCONTROLLER_UPDATELEDS:
                    state["colors"] = parse_update_leds(payload)
                    calls.append({"colors": state["colors"]})
                    save()
                else:
                    raise SystemExit(f"unexpected packet {packet_id}")
PY
server_pid=$!

for _ in {1..40}; do
  if [[ -s "$state" ]]; then
    break
  fi
  sleep 0.05
done

export RATVANTAGE_OPENRGB_SDK_HOST=127.0.0.1
export RATVANTAGE_OPENRGB_SDK_PORT="$port"

before="$("$helper" snapshot openrgb-sdk:/usr/bin/openrgb)"
python3 - <<'PY' "$before"
import json
import sys
snapshot = json.loads(sys.argv[1])
assert snapshot["active_mode"] == "Direct", snapshot
assert snapshot["colors"]["left_side"] == "#000000", snapshot
PY

"$helper" write openrgb-sdk:/usr/bin/openrgb '{"effect":"Breathing","colors":{"left_side":"#ff0000","left_center":"#00ff00","right_center":"#0000ff","right_side":"#ffffff"},"brightness":75,"speed":30}'
after="$("$helper" snapshot openrgb-sdk:/usr/bin/openrgb)"
python3 - <<'PY' "$after"
import json
import sys
snapshot = json.loads(sys.argv[1])
assert snapshot["active_mode"] == "Breathing", snapshot
assert snapshot["colors"]["right_side"] == "#FFFFFF", snapshot
PY

"$helper" restore openrgb-sdk:/usr/bin/openrgb "$before"
restored="$("$helper" snapshot openrgb-sdk:/usr/bin/openrgb)"
python3 - <<'PY' "$restored" "$calls"
import json
import pathlib
import sys
snapshot = json.loads(sys.argv[1])
calls = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert snapshot["active_mode"] == "Direct", snapshot
assert snapshot["colors"]["left_center"] == "#000000", snapshot
assert calls == [
    {"mode": "Breathing"},
    {"colors": ["FF0000", "00FF00", "0000FF", "FFFFFF"]},
    {"mode": "Direct"},
    {"colors": ["000000", "000000", "000000", "000000"]},
], calls
PY

kill "$server_pid" >/dev/null 2>&1 || true
wait "$server_pid" >/dev/null 2>&1 || true

echo "openrgb-keyboard-rgb-sdk-helper tests passed"
