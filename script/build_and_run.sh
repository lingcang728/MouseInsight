#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="Mouse Insight"
BUNDLE_ID="com.linc.mouseinsight"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$ROOT_DIR"
pkill -x "$APP_NAME" >/dev/null 2>&1 || true

BUILD_ARGS=()
BUILD_VARIANT="release"
if [[ "$MODE" == "--debug" || "$MODE" == "debug" ]]; then
  BUILD_ARGS+=(--debug)
  BUILD_VARIANT="debug"
fi

if [[ ${#BUILD_ARGS[@]} -gt 0 ]]; then
  npm run tauri build -- "${BUILD_ARGS[@]}"
else
  npm run tauri build
fi

APP_BUNDLE="$ROOT_DIR/src-tauri/target/$BUILD_VARIANT/bundle/macos/$APP_NAME.app"
APP_BINARY="$APP_BUNDLE/Contents/MacOS/$APP_NAME"

if [[ ! -x "$APP_BINARY" ]]; then
  echo "built app binary not found: $APP_BINARY" >&2
  exit 1
fi

open_app() {
  /usr/bin/open -n "$APP_BUNDLE"
}

case "$MODE" in
  run)
    open_app
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    open_app
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --telemetry|telemetry)
    open_app
    /usr/bin/log stream --info --style compact --predicate "subsystem == \"$BUNDLE_ID\""
    ;;
  --verify|verify)
    open_app
    sleep 2
    pgrep -x "$APP_NAME" >/dev/null
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
