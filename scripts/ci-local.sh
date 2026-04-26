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

command -v xvfb-run >/dev/null 2>&1 || {
  echo "missing xvfb-run; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v import >/dev/null 2>&1 || {
  echo "missing import; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v desktop-file-validate >/dev/null 2>&1 || {
  echo "missing desktop-file-validate; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

command -v appstreamcli >/dev/null 2>&1 || {
  echo "missing appstreamcli; run: scripts/install-dev-deps-fedora.sh" >&2
  exit 1
}

rust_minor="$(rustc --version | awk '{print $2}' | cut -d. -f2)"
if (( rust_minor < 92 )); then
  echo "rustc 1.92+ required for gtk-rs; current: $(rustc --version)" >&2
  echo "run: rustup toolchain install stable" >&2
  exit 1
fi

need_pkg_config gtk4 "scripts/install-dev-deps-fedora.sh"
need_pkg_config libadwaita-1 "scripts/install-dev-deps-fedora.sh"

cargo fmt --all --check
cargo test --workspace
xvfb-run -a cargo test -p legion-control-ui --features gtk-ui --test gtk_shell
cargo clippy --all-targets --all-features -- -D warnings
scripts/validate-packaging.sh
fixture_tmp="$(mktemp -d)"
trap 'rm -rf "$fixture_tmp"' EXIT
scripts/capture-sysfs-fixture.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/captured" >/tmp/ratvantage-fixture-capture.txt
scripts/capture-compat-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/compat" >/tmp/ratvantage-compat-capture.txt
scripts/capture-write-validation-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --output "$fixture_tmp/write-validation" \
  --skip-compat-bundle \
  --skip-tray-smoke >/tmp/ratvantage-write-validation.txt
scripts/run-local-session-app.sh \
  --frontend status \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed >/tmp/ratvantage-local-session-status.txt
scripts/capture-gtk-smoke-report.sh \
  --sysfs-root tests/fixtures/sysfs-82wm-confirmed \
  --pages status,battery \
  --output "$fixture_tmp/gtk-smoke" >/tmp/ratvantage-gtk-smoke.txt
cargo run -p legion-probe -- --json --sysfs-root "$fixture_tmp/captured" >/tmp/ratvantage-captured-probe.json
cargo run -p legion-probe -- --json --sysfs-root tests/fixtures/sysfs-82wm-confirmed >/tmp/ratvantage-probe.json
cargo run -p legion-control-daemon -- --dry-run >/tmp/ratvantage-daemon.txt

echo "local CI passed"
