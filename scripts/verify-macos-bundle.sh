#!/usr/bin/env bash
# Verify a signed/notarized Mouse Insight .app and optional .dmg.
# Usage: verify-macos-bundle.sh <App.app> [App.dmg]
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <App.app> [App.dmg]" >&2
  exit 2
fi

APP=$1
DMG=${2:-}

if [[ ! -d $APP ]]; then
  echo "missing app bundle: $APP" >&2
  exit 1
fi

echo "== codesign --verify =="
codesign --verify --deep --strict --verbose=2 "$APP"

echo "== public signing metadata =="
codesign -dv --verbose=4 "$APP" 2>&1 | awk '/Authority|TeamIdentifier|^Identifier=|^Format=|^Runtime=|^Signature=|^Flags=/' 

echo "== Gatekeeper (spctl) =="
spctl --assess --type execute --verbose "$APP"

echo "== stapler (app) =="
xcrun stapler validate "$APP"

if [[ -n $DMG ]]; then
  if [[ ! -f $DMG ]]; then
    echo "missing dmg: $DMG" >&2
    exit 1
  fi
  echo "== stapler (dmg) =="
  xcrun stapler validate "$DMG"
  echo "== Gatekeeper (dmg) =="
  spctl --assess --type open --context context:primary-signature --verbose "$DMG"
fi

echo "OK: signature, Gatekeeper, and stapling checks passed"
