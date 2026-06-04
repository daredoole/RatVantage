#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/status-keyboard-rgb-openrgb-bridge-evidence.sh [options]

Summarize OpenRGB keyboard RGB bridge evidence state.

Options:
  --root <dir>        Validation root. Default: target/validation.
  --readiness <path>  Optional OpenRGB readiness JSON or directory.
  --sdk <path>        Optional OpenRGB SDK evidence JSON or directory.
  --sdk-write <path>  Optional OpenRGB SDK write evidence JSON or directory.
  --json              Print structured JSON instead of concise text.
  -h, --help          Show this help.
EOF
}

root="target/validation"
readiness_path=""
sdk_path=""
sdk_write_path=""
json=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      root="${2:?missing value for --root}"
      shift 2
      ;;
    --readiness)
      readiness_path="${2:?missing value for --readiness}"
      shift 2
      ;;
    --sdk)
      sdk_path="${2:?missing value for --sdk}"
      shift 2
      ;;
    --sdk-write)
      sdk_write_path="${2:?missing value for --sdk-write}"
      shift 2
      ;;
    --json)
      json=1
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

python3 - "$root" "$readiness_path" "$sdk_path" "$sdk_write_path" "$json" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
readiness_arg = sys.argv[2]
sdk_arg = sys.argv[3]
sdk_write_arg = sys.argv[4]
json_mode = bool(int(sys.argv[5]))

def load_bundle(slug):
    path = root / slug / "openrgb-keyboard-rgb-bridge-evidence.json"
    if not path.exists():
        return {
            "slug": slug,
            "exists": False,
            "path": str(path),
            "status": "missing",
            "promotable": False,
        }
    try:
        report = json.loads(path.read_text())
    except Exception as error:
        return {
            "slug": slug,
            "exists": True,
            "path": str(path),
            "status": "invalid_json",
            "error": str(error),
            "promotable": False,
        }

    readback = report.get("readback") or {}
    result = report.get("result") or {}
    request = report.get("request") or {}
    blockers = result.get("promotion_blockers") or []
    promotable = bool(
        report.get("execute")
        and result.get("status") == "executed"
        and readback.get("mode_readback_matches")
        and readback.get("restore_mode_matches")
        and readback.get("color_readback_supported")
        and result.get("backend_ready_evidence")
        and not blockers
    )
    return {
        "slug": slug,
        "exists": True,
        "path": str(path),
        "status": result.get("status") or "unknown",
        "device": request.get("device"),
        "before_mode": readback.get("before_mode"),
        "after_mode": readback.get("after_mode"),
        "restored_mode": readback.get("restored_mode"),
        "color_readback_supported": bool(readback.get("color_readback_supported")),
        "backend_ready_evidence": bool(result.get("backend_ready_evidence")),
        "promotion_blockers": blockers,
        "promotable": promotable,
    }

def readiness_json_candidates(path):
    if not path:
        return []
    p = pathlib.Path(path)
    if p.is_file():
        return [p]
    if p.is_dir():
        return [
            p / "openrgb-keyboard-rgb-readiness.json",
            p / "openrgb-readiness" / "openrgb-keyboard-rgb-readiness.json",
        ]
    return [p]

def load_readiness():
    candidates = []
    if readiness_arg:
        candidates.extend(readiness_json_candidates(readiness_arg))
    candidates.extend([
        root / "keyboard-rgb-openrgb-readiness" / "openrgb-keyboard-rgb-readiness.json",
        root / "openrgb-readiness" / "openrgb-keyboard-rgb-readiness.json",
        root / "compatibility-bundle" / "openrgb-readiness" / "openrgb-keyboard-rgb-readiness.json",
    ])
    seen = set()
    for path in candidates:
        if path in seen:
            continue
        seen.add(path)
        if not path.exists():
            continue
        try:
            report = json.loads(path.read_text())
        except Exception as error:
            return {
                "exists": True,
                "path": str(path),
                "status": "invalid_json",
                "error": str(error),
                "ready_for_execute_evidence": False,
            }
        openrgb = report.get("openrgb") or {}
        access = report.get("linux_access") or {}
        candidate = bool(report.get("ratvantage", {}).get("openrgb_backend_candidate"))
        installed = bool(openrgb.get("installed"))
        detected = bool(openrgb.get("detects_lenovo_keyboard_rgb"))
        setup_recommended = bool(access.get("setup_recommended"))
        i2c_rw = bool(access.get("has_i2c_rw_access"))
        hidraw_rw = bool(access.get("has_hidraw_rw_access"))
        blockers = []
        if not installed:
            blockers.append("OpenRGB is not installed")
        if not detected:
            blockers.append("OpenRGB did not detect a Lenovo keyboard RGB device")
        if setup_recommended:
            blockers.append("OpenRGB access setup is recommended")
        if not i2c_rw:
            blockers.append("missing i2c read/write access")
        if not hidraw_rw:
            blockers.append("missing hidraw read/write access")
        if not candidate:
            blockers.append("OpenRGB backend candidate is false")
        return {
            "exists": True,
            "path": str(path),
            "status": "ok",
            "installed": installed,
            "detected": detected,
            "user": access.get("user"),
            "user_in_i2c_group": bool(access.get("user_in_i2c_group")),
            "has_i2c_rw_access": i2c_rw,
            "has_hidraw_rw_access": hidraw_rw,
            "setup_recommended": setup_recommended,
            "missing_access": access.get("missing_access") or [],
            "backend_candidate": candidate,
            "ready_for_execute_evidence": not blockers,
            "blockers": blockers,
        }
    return {
        "exists": False,
        "path": str(candidates[0]) if candidates else "",
        "status": "missing",
        "ready_for_execute_evidence": False,
        "blockers": ["capture OpenRGB readiness before execute evidence"],
    }

def sdk_json_candidates(path):
    if not path:
        return []
    p = pathlib.Path(path)
    if p.is_file():
        return [p]
    if p.is_dir():
        return [
            p / "openrgb-keyboard-rgb-sdk-evidence.json",
            p / "openrgb-sdk" / "openrgb-keyboard-rgb-sdk-evidence.json",
        ]
    return [p]

def load_sdk():
    candidates = []
    if sdk_arg:
        candidates.extend(sdk_json_candidates(sdk_arg))
    candidates.extend([
        root / "keyboard-rgb-openrgb-sdk" / "openrgb-keyboard-rgb-sdk-evidence.json",
        root / "openrgb-sdk" / "openrgb-keyboard-rgb-sdk-evidence.json",
        root / "compatibility-bundle" / "openrgb-sdk" / "openrgb-keyboard-rgb-sdk-evidence.json",
    ])
    seen = set()
    for path in candidates:
        if path in seen:
            continue
        seen.add(path)
        if not path.exists():
            continue
        try:
            report = json.loads(path.read_text())
        except Exception as error:
            return {
                "exists": True,
                "path": str(path),
                "status": "invalid_json",
                "error": str(error),
                "read_back_supported": False,
                "promotable": False,
            }
        sdk = report.get("sdk") or {}
        keyboard = report.get("keyboard") or {}
        result = report.get("result") or {}
        controllers = report.get("controllers") or []
        blockers = result.get("promotion_blockers") or []
        controller = keyboard.get("controller") or {}
        read_back_supported = bool(result.get("read_back_supported"))
        promotable = bool(
            result.get("status") == "ok"
            and read_back_supported
            and keyboard.get("detected")
            and not blockers
        )
        return {
            "exists": True,
            "path": str(path),
            "status": result.get("status") or "unknown",
            "connected": bool(sdk.get("connected")),
            "server_started": bool(sdk.get("server_started")),
            "protocol_version": sdk.get("protocol_version"),
            "controller_count": len(controllers),
            "keyboard_detected": bool(keyboard.get("detected")),
            "controller_name": controller.get("name"),
            "active_mode": controller.get("active_mode"),
            "color_count": len(controller.get("colors") or []),
            "read_back_supported": read_back_supported,
            "promotion_blockers": blockers,
            "promotable": promotable,
        }
    return {
        "exists": False,
        "path": str(candidates[0]) if candidates else "",
        "status": "missing",
        "read_back_supported": False,
        "promotable": False,
        "promotion_blockers": ["capture OpenRGB SDK read-back evidence"],
    }

def sdk_write_json_candidates(path):
    if not path:
        return []
    p = pathlib.Path(path)
    if p.is_file():
        return [p]
    if p.is_dir():
        return [
            p / "openrgb-keyboard-rgb-sdk-write-evidence.json",
            p / "openrgb-sdk-write" / "openrgb-keyboard-rgb-sdk-write-evidence.json",
        ]
    return [p]

def load_sdk_write():
    candidates = []
    if sdk_write_arg:
        candidates.extend(sdk_write_json_candidates(sdk_write_arg))
    candidates.extend([
        root / "keyboard-rgb-openrgb-sdk-write" / "openrgb-keyboard-rgb-sdk-write-evidence.json",
        root / "openrgb-sdk-write" / "openrgb-keyboard-rgb-sdk-write-evidence.json",
        root / "compatibility-bundle" / "openrgb-sdk-write" / "openrgb-keyboard-rgb-sdk-write-evidence.json",
    ])
    seen = set()
    for path in candidates:
        if path in seen:
            continue
        seen.add(path)
        if not path.exists():
            continue
        try:
            report = json.loads(path.read_text())
        except Exception as error:
            return {
                "exists": True,
                "path": str(path),
                "status": "invalid_json",
                "error": str(error),
                "promotable": False,
            }
        result = report.get("result") or {}
        readback = report.get("readback") or {}
        request = report.get("request") or {}
        blockers = result.get("promotion_blockers") or []
        after = readback.get("after") or {}
        requested_mode = request.get("mode") or request.get("effect")
        mode_readback_matches = bool(
            requested_mode
            and after.get("active_mode")
            and str(after.get("active_mode")).lower() == str(requested_mode).lower()
        )
        promotable = bool(
            report.get("execute")
            and result.get("status") == "executed"
            and result.get("sdk_write_ready_evidence")
            and mode_readback_matches
            and readback.get("color_readback_matches")
            and readback.get("restore_color_matches")
            and readback.get("restore_mode_matches")
            and not blockers
        )
        return {
            "exists": True,
            "path": str(path),
            "status": result.get("status") or "unknown",
            "requested_mode": requested_mode,
            "after_mode": after.get("active_mode"),
            "colors": request.get("colors") or [],
            "after_colors": after.get("colors") or [],
            "mode_readback_matches": mode_readback_matches,
            "color_readback_matches": bool(readback.get("color_readback_matches")),
            "restore_color_matches": bool(readback.get("restore_color_matches")),
            "restore_mode_matches": bool(readback.get("restore_mode_matches")),
            "sdk_write_ready_evidence": bool(result.get("sdk_write_ready_evidence")),
            "promotion_blockers": blockers,
            "promotable": promotable,
        }
    return {
        "exists": False,
        "path": str(candidates[0]) if candidates else "",
        "status": "missing",
        "promotable": False,
        "promotion_blockers": ["capture OpenRGB SDK write evidence"],
    }

dry = load_bundle("keyboard-rgb-openrgb-bridge-dry-run")
execute = load_bundle("keyboard-rgb-openrgb-bridge-execute")
readiness = load_readiness()
sdk = load_sdk()
sdk_write = load_sdk_write()
if execute["promotable"]:
    next_action = "promote only after production backend policy gates are added"
elif sdk_write["promotable"]:
    next_action = "wire real OpenRGB SDK helper and daemon policy gates"
elif sdk_write["exists"] and sdk_write.get("color_readback_matches") and not sdk_write.get("mode_readback_matches"):
    next_action = "prove OpenRGB SDK mode write/read-back before daemon promotion"
elif not readiness["ready_for_execute_evidence"]:
    next_action = "capture or fix OpenRGB readiness before execute evidence"
elif not dry["exists"]:
    next_action = "run dry-run evidence capture"
elif not execute["exists"]:
    next_action = "operator may run execute evidence capture"
elif not sdk["exists"]:
    next_action = "capture OpenRGB SDK read-back evidence"
elif sdk["promotable"] and not execute["promotable"]:
    next_action = "find an OpenRGB apply path that changes SDK mode/color read-back"
elif sdk["promotable"]:
    next_action = "review SDK read-back evidence before production backend promotion"
else:
    next_action = "review execute bundle and SDK read-back failures before promotion"

summary = {
    "root": str(root),
    "dry_run": dry,
    "execute": execute,
    "readiness": readiness,
    "sdk": sdk,
    "sdk_write": sdk_write,
    "next_action": next_action,
}

if json_mode:
    print(json.dumps(summary, indent=2, sort_keys=True))
else:
    for label, bundle in (("dry_run", dry), ("execute", execute)):
        if not bundle["exists"]:
            print(f"{label}=missing path={bundle['path']}")
            continue
        print(
            f"{label}=present status={bundle['status']} device={bundle.get('device') or 'unknown'} "
            f"before={bundle.get('before_mode') or 'unknown'} after={bundle.get('after_mode') or 'none'} "
            f"restored={bundle.get('restored_mode') or 'none'} color_readback={str(bundle.get('color_readback_supported')).lower()} "
            f"backend_ready={str(bundle.get('backend_ready_evidence')).lower()} promotable={str(bundle.get('promotable')).lower()} "
            f"blockers={len(bundle.get('promotion_blockers') or [])}"
        )
    if readiness["exists"]:
        print(
            f"readiness=present installed={str(readiness.get('installed')).lower()} "
            f"detected={str(readiness.get('detected')).lower()} "
            f"i2c_rw={str(readiness.get('has_i2c_rw_access')).lower()} "
            f"hidraw_rw={str(readiness.get('has_hidraw_rw_access')).lower()} "
            f"setup_recommended={str(readiness.get('setup_recommended')).lower()} "
            f"ready_for_execute={str(readiness.get('ready_for_execute_evidence')).lower()} "
            f"blockers={len(readiness.get('blockers') or [])}"
        )
    else:
        print(f"readiness=missing path={readiness['path']}")
    if sdk["exists"]:
        print(
            f"sdk=present status={sdk['status']} connected={str(sdk.get('connected')).lower()} "
            f"protocol={sdk.get('protocol_version') or 'unknown'} controllers={sdk.get('controller_count') or 0} "
            f"keyboard_detected={str(sdk.get('keyboard_detected')).lower()} "
            f"read_back={str(sdk.get('read_back_supported')).lower()} "
            f"promotable={str(sdk.get('promotable')).lower()} blockers={len(sdk.get('promotion_blockers') or [])}"
        )
    else:
        print(f"sdk=missing path={sdk['path']}")
    if sdk_write["exists"]:
        print(
            f"sdk_write=present status={sdk_write['status']} "
            f"mode_readback={str(sdk_write.get('mode_readback_matches')).lower()} "
            f"color_readback={str(sdk_write.get('color_readback_matches')).lower()} "
            f"restore_color={str(sdk_write.get('restore_color_matches')).lower()} "
            f"restore_mode={str(sdk_write.get('restore_mode_matches')).lower()} "
            f"color_write_ready={str(sdk_write.get('sdk_write_ready_evidence')).lower()} "
            f"promotable={str(sdk_write.get('promotable')).lower()} "
            f"blockers={len(sdk_write.get('promotion_blockers') or [])}"
        )
    else:
        print(f"sdk_write=missing path={sdk_write['path']}")
    print(f"next_action={next_action}")
PY
