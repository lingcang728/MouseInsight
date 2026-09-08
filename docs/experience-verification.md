# Experience and input reliability verification

## Scope

- Baseline: local `main` and `origin/main` both `1eb5d81`, ahead/behind `0/0` after fetch.
- Existing untracked `release/` and the running portable app are preserved.
- Offline application: JSON configuration, no database or remote mapping service. Network changes concern explicit GitHub update checks only.

## Changes

- Warm neutral/forest palettes, two-column workspace, responsive mapping editor, direct button creation and platform-aware shortcut presets.
- Input flash/ring animations use transform/opacity; idle has no animation loop. Reduced-motion preference disables motion. Card selection does not rebuild form controls; saves are serialized and acknowledge persistence.
- Fix stale legacy keys after re-recording or clearing a slot, preserving right Alt through editing and mode transitions. Fix removal of one recorded key unintentionally removing the remaining released chord.
- Windows resolves generic Ctrl/Alt/Shift with extended/scan metadata and sends side-specific modifiers with scan codes.
- macOS carries modifier flags per event, tracks left/right ownership independently, includes physically held modifiers, checks side-specific hardware state, and recovers disabled event taps in a paused state. Startup permission/monitor failures are visible.
- Both hooks retain ownership of swallowed down/up pairs through pause/config changes. Identification cannot fire the old mapped shortcut. Recording suspends mapping injection. Edge overflow schedules emergency release on the priority control channel instead of silently losing key-up.
- Configuration writes happen off the WebView thread, serialize against current configuration and activate mappings only after persistence. Existing atomic replacement and backup are retained.
- Update metadata uses one request, an 8-second abort, concurrent-request deduplication and a bounded 5-minute successful-result cache. Semantic prerelease ordering is tested. Only fixed project release URLs can open.
- CI uses Node 24, matching package.json.

## Local evidence

- Frontend production build and 41 logic/network regression tests pass.
- 48 Windows Rust tests pass, including right-Alt scan-code construction, stale dual keys, recorded chord editing and saturated input queue release.
- Windows release compilation passed. Subsequent changes are verified in the final native debug review build and CI packaging.
- `scripts/verify-ui.py` tests an actual isolated Tauri WebView2 via CDP, without a mock bridge. It verifies right Alt recording/persistence/reload, cancellation, clearing the last key, presets, stable select DOM, listening cancellation, dialog focus, light/dark, 1180x760 and 920x620 layouts, reduced motion and disk contents. No page errors.
- Native review configuration lives in a temporary portable directory with a separate application identifier; user configuration is untouched. Test mappings stay paused.

## Reproduce native smoke

Use the existing Python Playwright installation. Build a review executable with a separate Tauri identifier, copy it into an empty temporary folder with a `.portable` file and `config.json` containing `{"schema_version":1,"theme":"light","paused":true,"autostart":false,"mappings":[]}`. Enable WebView2 CDP only for that test process via `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`, then run:

```powershell
python scripts/verify-ui.py --cdp http://127.0.0.1:9223 --output <temporary-evidence-directory>
```

Close the review process after testing; do not enable the debugging port for normal use.

## Verification boundaries

Windows tests exercise the real recording/save bridge and native input construction. They do not prove acceptance by every target app, elevated process or keyboard layout (notably AltGr layouts). macOS native build and unit tests run in CI; physical mouse/keyboard, Accessibility grant/revoke, left/right Option, both Shift keys, event-tap recovery and target-app acceptance still require a Mac. No numerical end-to-end latency or RSS reduction is claimed without a controlled before/after workload. The UI tests are paused to avoid altering the user's active desktop input.

Native API references: [Microsoft KEYBDINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput), [Microsoft KBDLLHOOKSTRUCT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-kbdllhookstruct), [Apple event source states](https://developer.apple.com/documentation/coregraphics/cgeventsourcestateid).
