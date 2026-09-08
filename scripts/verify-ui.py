"""Smoke the real Tauri WebView2 via an explicitly enabled local CDP port.

Requires an isolated portable review build with empty, paused config. Uses the
machine's existing Python Playwright; does not install a browser or mock Tauri.
Usage: python scripts/verify-ui.py --cdp http://127.0.0.1:9223 --output <directory>
"""
import argparse
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

parser = argparse.ArgumentParser()
parser.add_argument('--cdp', required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)

with sync_playwright() as playwright:
    browser = playwright.chromium.connect_over_cdp(args.cdp)
    page = next(page for context in browser.contexts for page in context.pages if 'tauri' in page.url)
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    invoke = lambda command: page.evaluate("command => window.__TAURI_INTERNALS__.invoke(command)", command)
    before = invoke('get_snapshot')
    assert before['config']['paused'] and not before['config']['mappings'], 'Use an empty, paused, isolated review config'
    expect(page.locator('#engine-status')).to_have_text('映射已暂停')
    expect(page.locator('#alert-banner')).to_be_hidden()

    # Real recording commands, preset events, save acknowledgement and on-disk config.
    page.locator('[data-add-button="xbutton1"]').click()
    expect(page.get_by_role('dialog')).to_be_visible()
    page.locator('[data-key="RAlt"]').click()
    expect(page.locator('#record-keys')).to_contain_text('Right Alt')
    page.locator('#record-ok').click()
    expect(page.get_by_role('dialog')).to_be_hidden()
    expect(page.locator('#save-status')).to_contain_text('已保存')
    assert invoke('get_snapshot')['config']['mappings'][0]['keys'] == ['RAlt']

    # Editing does not retain the old chord; cancelling leaves committed data intact.
    page.locator('.map [data-act="record"][data-slot="tap"]').click()
    page.locator('[data-preset="copy"]').click()
    expect(page.locator('#record-keys')).to_contain_text('Ctrl')
    page.locator('#record-cancel').click()
    assert invoke('get_snapshot')['config']['mappings'][0]['keys'] == ['RAlt']

    # Remove the final key, ensuring no legacy fallback resurrects it after reload.
    page.locator('.map [data-remove-key="RAlt"]').click()
    expect(page.locator('#save-status')).to_contain_text('已保存')
    assert invoke('get_snapshot')['config']['mappings'][0]['keys'] == []
    page.reload()
    expect(page.locator('.map')).to_have_count(1)
    expect(page.locator('.map .combo').first).to_have_text('空')

    # Preset plus a second mapping; selecting its native select must not replace DOM.
    page.locator('.map [data-act="record"][data-slot="tap"]').click()
    page.locator('[data-preset="copy"]').click()
    expect(page.locator('#record-keys')).to_contain_text('Ctrl')
    page.locator('#record-ok').click()
    expect(page.get_by_role('dialog')).to_be_hidden()
    page.locator('[data-add-button="xbutton2"]').click()
    page.locator('[data-preset="paste"]').click()
    expect(page.locator('#record-keys')).to_contain_text('V')
    page.locator('#record-ok').click()
    expect(page.locator('.map')).to_have_count(2)
    expect(page.get_by_role('dialog')).to_be_hidden()
    expect(page.locator('#save-status')).to_contain_text('已保存')
    first_select = page.locator('.map select').first.element_handle()
    first_select.click()
    page.keyboard.press('Escape')
    assert first_select.evaluate('(element) => element.isConnected')

    # Listening cancellation, keyboard focus containment and both themes.
    page.locator('#btn-listen').click()
    expect(page.locator('#btn-listen')).to_have_text('取消识别')
    page.locator('#btn-listen').click()
    assert invoke('get_snapshot')['listening'] is False
    page.locator('[data-add-button="middle"]').click()
    page.keyboard.press('Tab')
    assert page.evaluate("!!document.activeElement.closest('[role=dialog]')")
    page.keyboard.press('Escape')
    expect(page.get_by_role('dialog')).to_be_hidden()
    page.locator('.stage').evaluate('(element) => element.scrollTop = 0')
    page.screenshot(path=str(args.output / 'light.png'))
    page.locator('#btn-theme').click()
    expect(page.locator('html')).to_have_attribute('data-theme', 'dark')
    page.screenshot(path=str(args.output / 'dark.png'))
    page.locator('#btn-theme').click()

    # CDP emulates the content viewport; native window dimensions are unchanged.
    cdp = page.context.new_cdp_session(page)
    for width, height in [(1180, 760), (920, 620)]:
        cdp.send('Emulation.setDeviceMetricsOverride', {'width': width, 'height': height, 'deviceScaleFactor': 1, 'mobile': False})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), f'Horizontal overflow at {width}'
        assert page.locator('.stage').evaluate('(element) => element.scrollWidth <= element.clientWidth'), f'Stage overflow at {width}'
    page.screenshot(path=str(args.output / 'compact.png'))
    cdp.send('Emulation.clearDeviceMetricsOverride')
    cdp.send('Emulation.setEmulatedMedia', {'features': [{'name': 'prefers-reduced-motion', 'value': 'reduce'}]})
    page.locator('[data-add-button="middle"]').click()
    assert page.locator('.record-card').evaluate('(element) => getComputedStyle(element).animationName') == 'none'
    page.locator('#record-cancel').click()
    assert errors == [], errors
    after = invoke('get_snapshot')
    config = Path(after['config_dir']) / 'config.json'
    assert json.loads(config.read_text(encoding='utf-8'))['mappings'] == after['config']['mappings']
    print(json.dumps({'result': 'passed', 'checks': ['native IPC recording/presets', 'right Alt roundtrip', 'cancel transaction', 'clear last key/reload', 'select identity', 'listen cancellation', 'dialog focus', 'light/dark', '1180/920 overflow', 'reduced motion', 'disk persistence'], 'page_errors': errors}, ensure_ascii=False))
    browser.close()
