#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$repo_root/scripts/openrgb-sdk-server-session.sh"
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

fake_openrgb="$tmp/openrgb"
cat >"$fake_openrgb" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
host="127.0.0.1"
port="6742"
while (($#)); do
  case "$1" in
    --server-host)
      host="$2"
      shift 2
      ;;
    --server-port)
      port="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
exec python3 - "$host" "$port" <<'PY'
import socket
import struct
import sys
import time

host, port = sys.argv[1], int(sys.argv[2])

def recv_exact(conn, size):
    data = b""
    while len(data) < size:
        chunk = conn.recv(size - len(data))
        if not chunk:
            return data
        data += chunk
    return data

with socket.socket() as server:
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind((host, port))
    server.listen(8)
    while True:
        conn, _ = server.accept()
        with conn:
            header = recv_exact(conn, 16)
            if len(header) != 16:
                continue
            magic, dev_idx, packet_id, size = struct.unpack("<4sIII", header)
            payload = recv_exact(conn, size)
            if magic == b"ORGB" and packet_id == 40:
                body = struct.pack("<I", 4)
                conn.sendall(b"ORGB" + struct.pack("<III", dev_idx, packet_id, len(body)) + body)
PY
EOF
chmod +x "$fake_openrgb"

export XDG_RUNTIME_DIR="$tmp/runtime"
export HOME="$tmp/home"
mkdir -p "$XDG_RUNTIME_DIR" "$HOME"
port="$(pick_port)"

stopped="$("$script" status --openrgb-bin "$fake_openrgb" --port "$port")"
grep -q "openrgb_sdk_server=stopped" <<<"$stopped"

started="$("$script" start --openrgb-bin "$fake_openrgb" --port "$port" --stability-wait 0.05)"
grep -q "openrgb_sdk_server=running" <<<"$started"
grep -q "port=$port" <<<"$started"

started_again="$("$script" start --openrgb-bin "$fake_openrgb" --port "$port")"
grep -q "openrgb_sdk_server=running" <<<"$started_again"

stopped_again="$("$script" stop --openrgb-bin "$fake_openrgb" --port "$port")"
grep -q "openrgb_sdk_server=stopped" <<<"$stopped_again"

echo "openrgb-sdk-server-session tests passed"
