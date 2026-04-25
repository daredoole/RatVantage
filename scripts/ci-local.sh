#!/usr/bin/env bash
set -euo pipefail

need_pkg_config() {
  local package="$1"
  local installer="$2"

  if ! pkg-config --exists "$package"; then
    echo "missing pkg-config package: $package" >&2
    echo "run: $installer" >&2
    exit 1
  fi
}

command -v dbus-daemon >/dev/null 2>&1 || {
  echo "missing dbus-daemon; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

need_pkg_config gtk4 "scripts/install-dev-deps-fedora.sh"
need_pkg_config libadwaita-1 "scripts/install-dev-deps-fedora.sh"

cargo fmt --all --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed >/tmp/ratvantage-probe.json
cargo run -p legion-control-daemon -- --dry-run >/tmp/ratvantage-daemon.txt

echo "local CI passed"
