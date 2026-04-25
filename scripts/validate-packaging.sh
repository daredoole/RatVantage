#!/usr/bin/env bash
set -euo pipefail

desktop-file-validate data/desktop/org.ratvantage.LegionControl.desktop
appstreamcli validate --no-net data/metainfo/org.ratvantage.LegionControl.metainfo.xml
unit_tmp="$(mktemp --suffix=.service)"
trap 'rm -f "$unit_tmp"' EXIT
sed 's#^ExecStart=.*#ExecStart=/bin/true#' \
  data/systemd/legion-control-daemon.service >"$unit_tmp"
systemd-analyze verify "$unit_tmp"
python3 - <<'PY'
from pathlib import Path
from xml.etree import ElementTree

for path in [
    "data/dbus/org.ratvantage.LegionControl1.conf",
    "data/polkit/org.ratvantage.LegionControl1.policy",
    "data/metainfo/org.ratvantage.LegionControl.metainfo.xml",
]:
    ElementTree.parse(Path(path))
PY

echo "packaging assets validated"
