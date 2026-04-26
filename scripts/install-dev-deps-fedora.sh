#!/usr/bin/env bash
set -euo pipefail

if ! command -v dnf >/dev/null 2>&1; then
  echo "dnf not found; this helper is for Fedora." >&2
  exit 1
fi

sudo dnf install -y \
  appstream \
  appstream-compose \
  dbus \
  dbus-devel \
  desktop-file-utils \
  glib2-devel \
  gtk4-devel \
  ImageMagick \
  libadwaita-devel \
  polkit-devel \
  pkgconf-pkg-config \
  rust \
  cargo \
  systemd-devel \
  systemd-rpm-macros \
  xorg-x11-server-Xvfb
