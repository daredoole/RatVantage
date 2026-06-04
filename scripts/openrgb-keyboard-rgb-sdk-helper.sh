#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  openrgb-keyboard-rgb-sdk-helper.sh snapshot <openrgb-sdk:path>
  openrgb-keyboard-rgb-sdk-helper.sh write <openrgb-sdk:path> <request-json>
  openrgb-keyboard-rgb-sdk-helper.sh restore <openrgb-sdk:path> <snapshot-json>

Connects to an already-running OpenRGB SDK server and operates on the detected
Lenovo keyboard controller. It never writes hidraw, i2c, sysfs, WMI, or EC.

Environment:
  RATVANTAGE_OPENRGB_SDK_HOST  Default: 127.0.0.1
  RATVANTAGE_OPENRGB_SDK_PORT  Default: 6742
EOF
}

if [[ $# -lt 2 ]]; then
  usage >&2
  exit 2
fi

action="$1"
backend_path="$2"
payload="${3:-}"

case "$action" in
  snapshot|write|restore) ;;
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

if [[ "$backend_path" != openrgb-sdk:* ]]; then
  echo "backend path must start with openrgb-sdk:" >&2
  exit 2
fi
if [[ "$action" != snapshot && -z "$payload" ]]; then
  echo "$action requires JSON payload" >&2
  exit 2
fi

python3 - "$action" "$backend_path" "$payload" "${RATVANTAGE_OPENRGB_SDK_HOST:-127.0.0.1}" "${RATVANTAGE_OPENRGB_SDK_PORT:-6742}" <<'PY'
import json
import re
import socket
import struct
import sys
import time

action, backend_path, payload, host, port = sys.argv[1:]
port = int(port)

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
            raise ValueError("SDK payload ended early")
        value = self.data[self.offset : self.offset + size]
        self.offset += size
        return value

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


def send_packet(sock, dev_idx, packet_id, body=b""):
    sock.sendall(b"ORGB" + struct.pack("<III", dev_idx, packet_id, len(body)) + body)


def recv_exact(sock, size):
    data = b""
    while len(data) < size:
        chunk = sock.recv(size - len(data))
        if not chunk:
            raise ConnectionError("OpenRGB SDK connection closed")
        data += chunk
    return data


def recv_packet(sock):
    header = recv_exact(sock, 16)
    magic, dev_idx, packet_id, size = struct.unpack("<4sIII", header)
    if magic != b"ORGB":
        raise ValueError(f"unexpected SDK packet magic {magic!r}")
    return dev_idx, packet_id, recv_exact(sock, size)


def recv_expected(sock, expected_packet_id, expected_dev_idx=None):
    deadline = time.time() + 5
    while True:
        dev_idx, packet_id, body = recv_packet(sock)
        if packet_id == expected_packet_id and (
            expected_dev_idx is None or dev_idx == expected_dev_idx
        ):
            return body
        if packet_id != DEVICE_LIST_UPDATED and time.time() >= deadline:
            raise ValueError(f"expected SDK packet {expected_packet_id}, got {packet_id}")


def parse_rgb(reader):
    r = reader.take(1)[0]
    g = reader.take(1)[0]
    b = reader.take(1)[0]
    reader.take(1)
    return f"#{r:02X}{g:02X}{b:02X}"


def pack_string(value):
    raw = value.encode("utf-8") + b"\x00"
    return struct.pack("<H", len(raw)) + raw


def normalize_hex(value):
    value = str(value).strip()
    if value.startswith("#"):
        value = value[1:]
    value = value.upper()
    if not re.fullmatch(r"[0-9A-F]{6}", value):
        raise ValueError(f"invalid RGB color {value!r}")
    return value


def pack_rgb(value):
    return bytes.fromhex(normalize_hex(value)) + b"\x00"


def zone_key(label):
    out = []
    previous_underscore = False
    for char in label.lower():
        if char.isalnum():
            out.append(char)
            previous_underscore = False
        elif not previous_underscore:
            out.append("_")
            previous_underscore = True
    return "".join(out).strip("_")


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
    reader.string()
    reader.i32()
    reader.u32()
    reader.u32()
    reader.u32()
    matrix_size = reader.u16()
    if matrix_size:
        reader.take(matrix_size)
    if version >= 4:
        for _ in range(reader.u16()):
            reader.string()
            reader.i32()
            reader.u32()
            reader.u32()


def parse_controller(body, version, index):
    reader = Reader(body)
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
    controller["modes"] = [parse_mode(reader, version) for _ in range(mode_count)]
    controller["active_mode_index"] = active_mode
    controller["active_mode"] = (
        controller["modes"][active_mode]["name"]
        if 0 <= active_mode < len(controller["modes"])
        else None
    )
    for _ in range(reader.u16()):
        parse_zone(reader, version)
    leds = []
    for _ in range(reader.u16()):
        leds.append({"name": reader.string(), "value": reader.u32()})
    controller["leds"] = leds
    controller["colors"] = [parse_rgb(reader) for _ in range(reader.u16())]
    return controller


def snapshot_from_controller(controller):
    colors = {}
    for led, color in zip(controller.get("leds") or [], controller.get("colors") or []):
        colors[zone_key(led["name"])] = color
    return {"active_mode": controller.get("active_mode") or "", "colors": colors}


def find_keyboard(controllers):
    for controller in controllers:
        haystack = " ".join(
            str(controller.get(key, "")) for key in ("name", "vendor", "description")
        ).lower()
        if "lenovo" in haystack and ("keyboard" in haystack or "4-zone" in haystack):
            return controller
    raise ValueError("OpenRGB SDK did not report a Lenovo keyboard controller")


def find_mode(controller, name):
    for mode in controller.get("modes") or []:
        if str(mode.get("name", "")).lower() == str(name).lower():
            return mode
    raise ValueError(f"OpenRGB SDK mode {name!r} was not advertised")


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
    body = struct.pack("<i", int(mode["value"])) + mode_data_payload(mode, version, colors)
    return struct.pack("<I", len(body) + 4) + body


def update_leds_payload(colors):
    body = struct.pack("<H", len(colors)) + b"".join(pack_rgb(color) for color in colors)
    return struct.pack("<I", len(body) + 4) + body


def ordered_colors(controller, color_map):
    colors = []
    for led in controller.get("leds") or []:
        key = zone_key(led["name"])
        if key not in color_map:
            raise ValueError(f"missing color for SDK LED zone {key!r}")
        colors.append(color_map[key])
    return colors


with socket.create_connection((host, port), timeout=5) as sock:
    sock.settimeout(5)
    send_packet(sock, 0, REQUEST_PROTOCOL_VERSION, struct.pack("<I", CLIENT_PROTOCOL_VERSION))
    body = recv_expected(sock, REQUEST_PROTOCOL_VERSION)
    version = min(struct.unpack("<I", body[:4])[0], CLIENT_PROTOCOL_VERSION)
    send_packet(sock, 0, REQUEST_CONTROLLER_COUNT)
    body = recv_expected(sock, REQUEST_CONTROLLER_COUNT)
    count = struct.unpack("<I", body[:4])[0]
    controllers = []
    for index in range(count):
        req = struct.pack("<I", version) if version >= 1 else b""
        send_packet(sock, index, REQUEST_CONTROLLER_DATA, req)
        controllers.append(parse_controller(recv_expected(sock, REQUEST_CONTROLLER_DATA, index), version, index))
    keyboard = find_keyboard(controllers)

    if action == "snapshot":
        print(json.dumps(snapshot_from_controller(keyboard), sort_keys=True))
    elif action == "write":
        request = json.loads(payload)
        mode = find_mode(keyboard, request["effect"])
        colors = ordered_colors(keyboard, request["colors"])
        send_packet(sock, keyboard["index"], RGBCONTROLLER_UPDATEMODE, update_mode_payload(mode, version, colors[: len(mode.get("colors") or [])] or None))
        send_packet(sock, keyboard["index"], RGBCONTROLLER_UPDATELEDS, update_leds_payload(colors))
    elif action == "restore":
        snapshot = json.loads(payload)
        mode = find_mode(keyboard, snapshot["active_mode"])
        colors = ordered_colors(keyboard, snapshot["colors"])
        send_packet(sock, keyboard["index"], RGBCONTROLLER_UPDATEMODE, update_mode_payload(mode, version))
        send_packet(sock, keyboard["index"], RGBCONTROLLER_UPDATELEDS, update_leds_payload(colors))
PY
