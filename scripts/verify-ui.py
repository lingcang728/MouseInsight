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
    page.bring_to_front()
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

    # Card-only interactions must never mutate configuration or select labels.
    saved = invoke('get_snapshot')['config']['mappings']
    page.locator('.map:visible [data-act="collapse"]').click()
    expect(page.locator('.map:visible .gesture').first).to_be_hidden()
    page.locator('.map:visible [data-act="collapse"]').click()
    expect(page.locator('.map:visible .gesture').first).to_be_visible()
    old_id = page.locator('.map:visible').get_attribute('data-id')
    page.locator('#deck-next').click()
    expect(page.locator('.map:visible')).not_to_have_attribute('data-id', old_id)
    page.locator('#deck-prev').click()
    expect(page.locator('.map:visible')).to_have_attribute('data-id', old_id)
    grip = page.locator('.map:visible .card-grip').bounding_box()
    page.mouse.move(grip['x'] + 15, grip['y'] + 8)
    page.mouse.down()
    page.mouse.move(grip['x'] + 130, grip['y'] + 8, steps=8)
    page.mouse.up()
    expect(page.locator('.map:visible')).not_to_have_attribute('data-id', old_id)
    page.locator('#save-status').dblclick()
    assert page.evaluate('getSelection().toString()') == ''
    assert invoke('get_snapshot')['config']['mappings'] == saved
    assert page.locator('#zone-xbutton2').bounding_box()['y'] < page.locator('#zone-xbutton1').bounding_box()['y']
    page.locator('[data-add-button="xbutton1"]').click()
    expect(page.locator('.map:visible')).to_have_attribute('data-button', 'xbutton1')

    # Mid-drag overflow and interruptions: no stuck transform after screenshot/blur.
    def begin_drag(distance=70):
        grip = page.locator('.map:visible .card-grip')
        grip.scroll_into_view_if_needed()
        box = grip.bounding_box()
        page.mouse.move(box['x'] + 20, box['y'] + 8)
        page.mouse.down()
        page.mouse.move(box['x'] + 20 + distance, box['y'] + 8, steps=8)
        page.wait_for_timeout(40)
    def assert_reset():
        assert page.locator('.map:visible').evaluate('(el) => el.style.transform') == ''
        assert page.locator('.deck-preview').count() == 0
        assert page.locator('#maps').evaluate('(el) => !el.classList.contains("deck-moving")')
    begin_drag()
    assert page.locator('.deck-preview').count() == 1
    assert page.locator('.deck-preview').evaluate('(el) => parseFloat(getComputedStyle(el).filter.slice(5)) > 0')
    assert page.locator('.stage').evaluate('(el) => el.scrollWidth <= el.clientWidth')
    page.screenshot(path=str(args.output / 'drag-contained.png'))
    page.evaluate('window.dispatchEvent(new Event("blur"))')
    assert_reset()
    page.mouse.move(700, 350)
    assert_reset()
    page.mouse.up()
    begin_drag()
    page.evaluate('window.dispatchEvent(new KeyboardEvent("keydown", {key:"Meta"}))')
    assert_reset()
    page.mouse.up()
    begin_drag()
    page.locator('.map:visible .card-grip').evaluate('(el) => el.dispatchEvent(new PointerEvent("lostpointercapture", {bubbles:true}))')
    assert_reset()
    page.mouse.up()
    begin_drag(12)
    page.wait_for_timeout(120) # A slow, short drag should return to the same card.
    same_id = page.locator('.map:visible').get_attribute('data-id')
    page.mouse.up()
    page.wait_for_timeout(220)
    assert_reset()
    expect(page.locator('.map:visible')).to_have_attribute('data-id', same_id)
    assert invoke('get_snapshot')['config']['mappings'] == saved

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
    if page.locator('html').get_attribute('data-theme') != 'light':
        page.locator('#btn-theme').click()
    expect(page.locator('html')).to_have_attribute('data-theme', 'light')
    page.wait_for_timeout(250)
    page.locator('.stage').evaluate('(element) => element.scrollTop = 0')
    page.screenshot(path=str(args.output / 'light.png'))
    page.locator('#btn-theme').click()
    expect(page.locator('html')).to_have_attribute('data-theme', 'dark')
    page.wait_for_timeout(250)  # Let the theme color transitions settle before visual QA.
    page.screenshot(path=str(args.output / 'dark.png'))
    page.locator('#btn-theme').click()

    page.wait_for_timeout(250)
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
    # Three-card wraparound and cancellation while a settle animation is running.
    cdp.send('Emulation.setEmulatedMedia', {'features': [{'name': 'prefers-reduced-motion', 'value': 'no-preference'}]})
    page.locator('[data-add-button="middle"]').click()
    page.locator('[data-preset="enter"]').click()
    page.locator('#record-ok').click()
    expect(page.get_by_role('dialog')).to_be_hidden()
    expect(page.locator('.map')).to_have_count(3)
    start_id = page.locator('.map:visible').get_attribute('data-id')
    for direction in [1, -1]:
        for _ in range(3):
            old_id = page.locator('.map:visible').get_attribute('data-id')
            begin_drag(180 * direction)
            assert page.locator('.stage').evaluate('(el) => el.scrollWidth <= el.clientWidth')
            page.mouse.up()
            expect(page.locator('.map:visible')).not_to_have_attribute('data-id', old_id)
            assert_reset()
        expect(page.locator('.map:visible')).to_have_attribute('data-id', start_id)
    page.locator('#deck-next').click()
    page.evaluate('window.dispatchEvent(new Event("blur"))')
    assert_reset()
    page.locator('#deck-next').click()
    expect(page.locator('.map:visible')).not_to_have_attribute('data-id', start_id)
    page.locator('.stage').evaluate('(el) => el.scrollTop = 0')
    assert errors == [], errors
    after = invoke('get_snapshot')
    config = Path(after['config_dir']) / 'config.json'
    assert json.loads(config.read_text(encoding='utf-8'))['mappings'] == after['config']['mappings']
    print(json.dumps({'result': 'passed', 'checks': ['native IPC recording/presets', 'right Alt roundtrip', 'cancel transaction', 'clear last key/reload', 'select identity', 'listen cancellation', 'dialog focus', 'light/dark', '1180/920 overflow', 'reduced motion', 'disk persistence', 'card collapse/navigation/swipe', 'double-click does not select or save', 'front/rear indicator order', 'mid-drag containment/blur', 'focus/capture/screenshot cancellation', 'short-drag return', 'three-card wraparound both directions', 'settle interruption recovery'], 'page_errors': errors}, ensure_ascii=False))
    browser.close()
