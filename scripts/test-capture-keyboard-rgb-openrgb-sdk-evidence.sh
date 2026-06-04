#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/capture-keyboard-rgb-openrgb-sdk-evidence.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

port="$(
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"

python3 - "$port" >"$tmp/fake-sdk-server.stdout" 2>"$tmp/fake-sdk-server.stderr" <<'PY' &
import socket
import struct
import sys

port = int(sys.argv[1])

REQUEST_CONTROLLER_COUNT = 0
REQUEST_CONTROLLER_DATA = 1
REQUEST_PROTOCOL_VERSION = 40
PROTOCOL_VERSION = 3


def pack_string(value):
    raw = value.encode("utf-8") + b"\x00"
    return struct.pack("<H", len(raw)) + raw


def rgb(value):
    value = value.lstrip("#")
    return bytes.fromhex(value) + b"\x00"


def mode(name, index, colors):
    out = pack_string(name)
    out += struct.pack("<iIIIII", index, 0, 0, 100, 0, 100)
    out += struct.pack("<IIIIII", 0, len(colors), 50, 50, 0, 0)
    out += struct.pack("<H", len(colors))
    out += b"".join(rgb(color) for color in colors)
    return out


def zone(name):
    return pack_string(name) + struct.pack("<iIIIH", 1, 4, 4, 4, 0)


def led(name):
    return pack_string(name) + struct.pack("<I", 0)


def controller_payload():
    modes = [
        mode("Direct", 0, ["#112233", "#445566", "#778899", "#AABBCC"]),
        mode("Breathing", 1, ["#FF0000"]),
    ]
    leds = [led("Left side"), led("Left center"), led("Right center"), led("Right side")]
    colors = [rgb("#112233"), rgb("#445566"), rgb("#778899"), rgb("#AABBCC")]
    body = struct.pack("<i", 3)
    body += pack_string("Lenovo 5 2023")
    body += pack_string("Lenovo")
    body += pack_string("Lenovo 4-Zone device")
    body += pack_string("fixture-version")
    body += pack_string("")
    body += pack_string("")
    body += struct.pack("<Hi", len(modes), 0)
    body += b"".join(modes)
    body += struct.pack("<H", 1) + zone("Keyboard")
    body += struct.pack("<H", len(leds)) + b"".join(leds)
    body += struct.pack("<H", len(colors)) + b"".join(colors)
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


with socket.socket() as server:
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(1)
    conn, _ = server.accept()
    with conn:
        for _ in range(3):
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
            else:
                raise SystemExit(3)
PY
server_pid=$!

"$script" --output "$tmp/sdk" --no-start-server --host 127.0.0.1 --port "$port" >/dev/null
wait "$server_pid"

python3 - "$tmp/sdk/openrgb-keyboard-rgb-sdk-evidence.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["result"]["status"] != "ok":
    raise SystemExit(report["result"])
if not report["result"]["read_back_supported"]:
    raise SystemExit("SDK read-back should be supported by fake controller")
keyboard = report["keyboard"]["controller"]
if keyboard["name"] != "Lenovo 5 2023":
    raise SystemExit(f"unexpected keyboard: {keyboard}")
if keyboard["active_mode"] != "Direct":
    raise SystemExit(f"unexpected active mode: {keyboard['active_mode']}")
if keyboard["colors"] != ["#112233", "#445566", "#778899", "#AABBCC"]:
    raise SystemExit(f"unexpected colors: {keyboard['colors']}")
if [led["name"] for led in keyboard["leds"]] != [
    "Left side",
    "Left center",
    "Right center",
    "Right side",
]:
    raise SystemExit(f"unexpected LED list: {keyboard['leds']}")
PY

delayed_port="$(
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"

python3 - "$delayed_port" >"$tmp/fake-delayed-sdk-server.stdout" 2>"$tmp/fake-delayed-sdk-server.stderr" <<'PY' &
import socket
import struct
import sys

port = int(sys.argv[1])

REQUEST_CONTROLLER_COUNT = 0
REQUEST_CONTROLLER_DATA = 1
REQUEST_PROTOCOL_VERSION = 40
PROTOCOL_VERSION = 3


def pack_string(value):
    raw = value.encode("utf-8") + b"\x00"
    return struct.pack("<H", len(raw)) + raw


def rgb(value):
    return bytes.fromhex(value.lstrip("#")) + b"\x00"


def mode(name, index, colors):
    out = pack_string(name)
    out += struct.pack("<iIIIII", index, 0, 0, 100, 0, 100)
    out += struct.pack("<IIIIII", 0, len(colors), 50, 50, 0, 0)
    out += struct.pack("<H", len(colors))
    out += b"".join(rgb(color) for color in colors)
    return out


def controller_payload():
    modes = [mode("Direct", 0, ["#010203"])]
    body = struct.pack("<i", 3)
    body += pack_string("Lenovo delayed keyboard")
    body += pack_string("Lenovo")
    body += pack_string("Lenovo 4-Zone device")
    body += pack_string("fixture-version")
    body += pack_string("")
    body += pack_string("")
    body += struct.pack("<Hi", len(modes), 0)
    body += b"".join(modes)
    body += struct.pack("<H", 1)
    body += pack_string("Keyboard") + struct.pack("<iIIIH", 1, 1, 1, 1, 0)
    body += struct.pack("<H", 1) + pack_string("Only LED") + struct.pack("<I", 0)
    body += struct.pack("<H", 1) + rgb("#010203")
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


count_requests = 0
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
            recv_exact(conn, size)
            if packet_id == REQUEST_PROTOCOL_VERSION:
                send(conn, 0, packet_id, struct.pack("<I", PROTOCOL_VERSION))
            elif packet_id == REQUEST_CONTROLLER_COUNT:
                count_requests += 1
                send(conn, 0, packet_id, struct.pack("<I", 0 if count_requests == 1 else 1))
            elif packet_id == REQUEST_CONTROLLER_DATA:
                send(conn, dev_idx, packet_id, controller_payload())
                break
            else:
                raise SystemExit(3)
PY
delayed_server_pid=$!

"$script" --output "$tmp/delayed-sdk" --no-start-server --host 127.0.0.1 --port "$delayed_port" >/dev/null
wait "$delayed_server_pid"

python3 - "$tmp/delayed-sdk/openrgb-keyboard-rgb-sdk-evidence.json" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text())
if report["result"]["status"] != "ok":
    raise SystemExit(report["result"])
if report["sdk"]["controller_count"] != 1:
    raise SystemExit(f"unexpected controller count: {report['sdk']}")
if report["sdk"]["controller_count_attempts"] < 2:
    raise SystemExit(f"expected retry attempts: {report['sdk']}")
if report["keyboard"]["controller"]["name"] != "Lenovo delayed keyboard":
    raise SystemExit(report["keyboard"])
PY

echo "capture-keyboard-rgb-openrgb-sdk-evidence tests passed"
