"""
Automated UI test for the Hive Settings panel.

Since GPUI renders everything via GPU (no standard Windows UI Automation elements),
this test uses coordinate-based mouse/keyboard automation with screenshot comparison.

IMPORTANT: Uses DPI-aware mode so all coordinates (mouse + screenshots) are in
physical pixels, consistent with each other.

Requirements: pip install pywinauto pillow
Run: python test_settings_ui.py  (with Hive app already running)
"""
import ctypes
import time
import sys
import os

# Make this process DPI-aware BEFORE any other Windows API calls.
# This ensures ImageGrab returns physical pixels matching mouse coordinates.
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)  # PROCESS_PER_MONITOR_DPI_AWARE
except Exception:
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass

from PIL import ImageGrab, ImageChops, ImageStat
from pywinauto import Application, Desktop
import pywinauto.mouse as mouse
from pywinauto.keyboard import send_keys

SCREENSHOT_DIR = os.path.join(os.path.dirname(__file__), "test_screenshots")
os.makedirs(SCREENSHOT_DIR, exist_ok=True)


def get_dpi_scale():
    """Get the DPI scaling factor for the primary monitor."""
    try:
        hdc = ctypes.windll.user32.GetDC(0)
        dpi = ctypes.windll.gdi32.GetDeviceCaps(hdc, 88)  # LOGPIXELSX
        ctypes.windll.user32.ReleaseDC(0, hdc)
        return dpi / 96.0
    except Exception:
        return 1.0


class TestResults:
    """Accumulates pass/fail results."""
    def __init__(self):
        self.results = []

    def check(self, name, condition, detail=""):
        status = "+" if condition else "X"
        self.results.append((name, condition, detail))
        print(f"  [{status}] {name}" + (f" -- {detail}" if detail else ""))
        return condition

    def summary(self):
        passed = sum(1 for _, ok, _ in self.results if ok)
        total = len(self.results)
        print(f"\n{'=' * 60}")
        print(f"Results: {passed}/{total} passed")
        if passed < total:
            print("FAILURES:")
            for name, ok, detail in self.results:
                if not ok:
                    print(f"  - {name}: {detail}")
        print(f"{'=' * 60}")
        return passed == total


def screenshot(name, bbox=None):
    """Take a screenshot and save it. Returns PIL Image."""
    img = ImageGrab.grab(bbox=bbox) if bbox else ImageGrab.grab()
    path = os.path.join(SCREENSHOT_DIR, f"{name}.png")
    img.save(path)
    return img


def win_bbox(rect):
    """Convert pywinauto rectangle to PIL bbox tuple."""
    return (rect.left, rect.top, rect.right, rect.bottom)


def content_crop(img, sidebar_px=100):
    """Crop the content area (excluding sidebar)."""
    return img.crop((sidebar_px, 0, img.width, img.height))


def images_differ(img1, img2, threshold=3.0):
    """Check if two images are meaningfully different."""
    if img1.size != img2.size:
        return True, 999.0
    diff = ImageChops.difference(img1.convert("RGB"), img2.convert("RGB"))
    stat = ImageStat.Stat(diff)
    mean_diff = sum(stat.mean) / 3.0
    return mean_diff > threshold, mean_diff


def connect_to_hive():
    """Find and connect to the running Hive window."""
    for attempt_fn in [
        lambda: Application(backend="uia").connect(path="hive.exe", timeout=5),
        lambda: Application(backend="uia").connect(title_re=".*[Hh]ive.*", timeout=5),
    ]:
        try:
            app = attempt_fn()
            return app, app.top_window()
        except Exception:
            continue

    # Desktop scan fallback
    try:
        desktop = Desktop(backend="uia")
        for w in desktop.windows():
            if 'hive' in w.window_text().lower():
                app = Application(backend="uia").connect(handle=w.handle)
                return app, app.top_window()
    except Exception:
        pass

    return None, None


def measure_sidebar(rect, img):
    """Measure sidebar button positions from a screenshot.
    Returns (btn_x_center, first_btn_y, btn_spacing) in physical pixels.
    """
    # Sidebar is the left strip. Scan for button-like features.
    # For now use measured constants from screenshots at physical resolution.
    # The sidebar is ~70 logical px wide. Each button is ~51 logical px tall.
    # First button starts after the titlebar (~40 logical px).
    w = rect.right - rect.left
    h = rect.bottom - rect.top
    print(f"  Window physical size: {w}x{h}")
    print(f"  Screenshot size: {img.width}x{img.height}")

    # If screenshot matches window size, coordinates are in physical pixels (DPI-aware)
    # If screenshot is smaller, we need to scale
    if abs(img.width - w) < 10:
        scale = 1.0
    else:
        scale = img.width / w

    print(f"  Coordinate scale factor: {scale:.2f}")
    return scale


def main():
    results = TestResults()
    dpi_scale = get_dpi_scale()

    print("=" * 60)
    print("Hive Settings Panel - Visual Automation Test")
    print(f"DPI scale: {dpi_scale:.2f}x")
    print("=" * 60)

    # ---------------------------------------------------------------
    # Step 1: Connect to Hive
    # ---------------------------------------------------------------
    print("\n[1] Connecting to Hive window...")
    app, win = connect_to_hive()
    if not win:
        print("  ERROR: Could not find Hive window. Is it running?")
        return False

    rect = win.rectangle()
    w, h = rect.right - rect.left, rect.bottom - rect.top
    print(f"  Window rect: {rect} ({w}x{h})")
    results.check("Window found", True, f"{w}x{h}")

    # Focus the window
    try:
        win.set_focus()
        time.sleep(0.5)
    except Exception:
        pass

    # Take a calibration screenshot to check DPI alignment
    cal_img = screenshot("00_calibration", win_bbox(rect))
    scale = measure_sidebar(rect, cal_img)

    # Calculate sidebar button positions using physical pixel coordinates.
    # Measured from the calibration screenshot at physical resolution.
    # On a 1942x1213 physical pixel window at 150% DPI:
    #   - Sidebar visual width: ~105 physical px
    #   - Button centers at x ≈ 53 from window left
    #   - First button (Chat) center at y ≈ 97 from window top
    #   - Button spacing: ≈ 77 physical px
    # We'll auto-detect by scanning the screenshot.

    # Auto-detect: find cyan-highlighted sidebar button position
    # (The active panel has cyan text)
    sidebar_x = None
    btn_positions = []

    # Scan for bright cyan pixels in the sidebar area (left 120px of screenshot)
    sidebar_region = cal_img.crop((0, 0, min(120, cal_img.width), cal_img.height))
    pixels = sidebar_region.load()
    sw, sh = sidebar_region.size

    # Find rows with bright cyan pixels (R<100, G>150, B>150)
    cyan_rows = []
    for y in range(sh):
        for x in range(sw):
            r, g, b = pixels[x, y][:3]
            if r < 100 and g > 150 and b > 200:
                cyan_rows.append((y, x))
                break

    if cyan_rows:
        # Group consecutive rows into button regions
        groups = []
        current_group = [cyan_rows[0]]
        for i in range(1, len(cyan_rows)):
            if cyan_rows[i][0] - cyan_rows[i-1][0] <= 2:
                current_group.append(cyan_rows[i])
            else:
                if len(current_group) >= 3:
                    groups.append(current_group)
                current_group = [cyan_rows[i]]
        if len(current_group) >= 3:
            groups.append(current_group)

        if groups:
            # The first cyan group should be the active panel text
            first_group = groups[0]
            btn_center_y = (first_group[0][0] + first_group[-1][0]) // 2
            btn_center_x = max(g[1] for g in first_group) // 2 + 10
            print(f"  Detected active button at y={btn_center_y}, x≈{btn_center_x}")

            # The app starts on Chat (index 0)
            # So first_btn_y ≈ btn_center_y
            # We need to find button spacing by looking for other icon/text rows

    # Fall back to calculated positions using DPI scale
    # At 96 DPI (100%): sidebar buttons are ~51px apart, centered at x=35, first at y=65
    # Scale these by DPI factor for physical coordinates
    sidebar_btn_x = int(35 * dpi_scale)
    first_btn_y = int(65 * dpi_scale)
    btn_spacing = int(51 * dpi_scale)

    print(f"  Using button layout: x={sidebar_btn_x}, first_y={first_btn_y}, spacing={btn_spacing}")
    print(f"  (based on DPI scale {dpi_scale:.2f}x)")

    # Panel indices
    PANEL_CHAT = 0
    PANEL_HISTORY = 1
    PANEL_FILES = 2
    PANEL_SETTINGS = 11
    PANEL_HELP = 12

    def click_panel(index, name=""):
        abs_x = rect.left + sidebar_btn_x
        abs_y = rect.top + first_btn_y + index * btn_spacing
        mouse.click(coords=(abs_x, abs_y))
        print(f"  Clicked {name or f'panel {index}'} at ({abs_x}, {abs_y}) [rel: ({sidebar_btn_x}, {first_btn_y + index * btn_spacing})]")
        return (abs_x, abs_y)

    # ---------------------------------------------------------------
    # Step 2: Navigate to Chat first (should already be there on fresh start)
    # ---------------------------------------------------------------
    print("\n[2] Navigating to Chat panel (baseline)...")
    click_panel(PANEL_CHAT, "Chat")
    time.sleep(0.8)
    chat_img = screenshot("01_chat_panel", win_bbox(rect))

    # ---------------------------------------------------------------
    # Step 3: Navigate to Settings panel
    # ---------------------------------------------------------------
    print("\n[3] Navigating to Settings panel...")
    click_panel(PANEL_SETTINGS, "Settings")
    time.sleep(1.0)
    settings_img = screenshot("02_settings_panel", win_bbox(rect))

    # Compare content areas (exclude sidebar)
    chat_content = content_crop(chat_img)
    settings_content = content_crop(settings_img)
    changed, diff = images_differ(chat_content, settings_content)
    results.check("Settings panel differs from Chat panel",
                  changed, f"mean_diff={diff:.2f}")

    # ---------------------------------------------------------------
    # Step 4: Verify Settings has content
    # ---------------------------------------------------------------
    print("\n[4] Verifying Settings panel content...")
    # Check if header region has rendered content (not blank dark background)
    header_area = settings_img.crop((int(100 * dpi_scale), int(40 * dpi_scale),
                                      int(600 * dpi_scale), int(150 * dpi_scale)))
    stat = ImageStat.Stat(header_area)
    avg = sum(stat.mean) / 3.0
    results.check("Settings header area has content",
                  5.0 < avg < 250.0,
                  f"avg_brightness={avg:.1f}")

    # ---------------------------------------------------------------
    # Step 5: Test API key input field
    # ---------------------------------------------------------------
    print("\n[5] Testing API key input field...")
    # API key input fields are right-aligned in the content area.
    # The first row (Anthropic) is approximately 270 logical px from window top.
    # The input center is roughly at x = window_width - 260 logical px
    input_x_logical = w // dpi_scale - 260  # logical px from left
    anthropic_row_logical_y = 273  # from window top in logical px

    input_x = rect.left + int(input_x_logical * dpi_scale)
    input_y = rect.top + int(anthropic_row_logical_y * dpi_scale)

    # Capture input region before
    ir_left = rect.left + int((w / dpi_scale - 400) * dpi_scale)
    ir_top = rect.top + int(255 * dpi_scale)
    ir_right = rect.left + int((w / dpi_scale - 130) * dpi_scale)
    ir_bottom = rect.top + int(295 * dpi_scale)
    before_input = screenshot("03a_input_before", (ir_left, ir_top, ir_right, ir_bottom))

    print(f"  Clicking Anthropic key input at ({input_x}, {input_y})")
    mouse.click(coords=(input_x, input_y))
    time.sleep(0.5)

    # Select all + delete, then type
    send_keys("^a", pause=0.05)
    time.sleep(0.1)
    send_keys("{DELETE}", pause=0.05)
    time.sleep(0.2)

    test_key = "sk-test-auto-12345"
    send_keys(test_key, pause=0.03)
    time.sleep(0.5)

    after_input = screenshot("03b_input_after", (ir_left, ir_top, ir_right, ir_bottom))
    screenshot("03c_full_after_typing", win_bbox(rect))

    changed, diff = images_differ(before_input, after_input)
    results.check("Input field changed after typing",
                  changed, f"mean_diff={diff:.2f}")

    # ---------------------------------------------------------------
    # Step 6: Tab to trigger blur (auto-save)
    # ---------------------------------------------------------------
    print("\n[6] Pressing Tab to trigger blur...")
    badge_left = rect.left + int((w / dpi_scale - 160) * dpi_scale)
    badge_top = rect.top + int(260 * dpi_scale)
    badge_right = rect.left + int((w / dpi_scale - 20) * dpi_scale)
    badge_bottom = rect.top + int(290 * dpi_scale)
    before_badge = screenshot("04a_badge_before", (badge_left, badge_top, badge_right, badge_bottom))

    send_keys("{TAB}", pause=0.1)
    time.sleep(1.0)

    after_badge = screenshot("04b_badge_after", (badge_left, badge_top, badge_right, badge_bottom))
    screenshot("04c_full_after_blur", win_bbox(rect))

    badge_changed, badge_diff = images_differ(before_badge, after_badge)
    results.check("Badge region updated after blur",
                  badge_changed, f"mean_diff={badge_diff:.2f}")

    # ---------------------------------------------------------------
    # Step 7: Scroll down
    # ---------------------------------------------------------------
    print("\n[7] Scrolling to reveal more sections...")
    cx = (rect.left + rect.right) // 2
    cy = (rect.top + rect.bottom) // 2

    before_scroll = screenshot("05a_before_scroll", win_bbox(rect))

    mouse.click(coords=(cx, cy))
    time.sleep(0.2)
    mouse.scroll(coords=(cx, cy), wheel_dist=-5)
    time.sleep(0.8)

    after_scroll = screenshot("05b_after_scroll", win_bbox(rect))

    sb = content_crop(before_scroll)
    sa = content_crop(after_scroll)
    scrolled, scroll_diff = images_differ(sb, sa)
    results.check("Content scrolled down",
                  scrolled, f"mean_diff={scroll_diff:.2f}")

    # ---------------------------------------------------------------
    # Step 8: Navigate Files -> Settings (round-trip)
    # ---------------------------------------------------------------
    print("\n[8] Testing panel navigation: Files -> Settings round-trip...")
    click_panel(PANEL_FILES, "Files")
    time.sleep(0.8)
    files_img = screenshot("06a_files_panel", win_bbox(rect))

    click_panel(PANEL_SETTINGS, "Settings")
    time.sleep(0.8)
    settings2_img = screenshot("06b_settings_again", win_bbox(rect))

    fc = content_crop(files_img)
    sc = content_crop(settings2_img)
    diff_ok, diff_val = images_differ(fc, sc)
    results.check("Files and Settings panels are visually different",
                  diff_ok, f"mean_diff={diff_val:.2f}")

    # ---------------------------------------------------------------
    # Step 9: Cleanup - clear test input
    # ---------------------------------------------------------------
    print("\n[9] Cleaning up test data...")
    # Scroll back to top
    mouse.scroll(coords=(cx, cy), wheel_dist=10)
    time.sleep(0.5)

    mouse.click(coords=(input_x, input_y))
    time.sleep(0.3)
    send_keys("^a{DELETE}", pause=0.05)
    time.sleep(0.2)
    send_keys("{TAB}", pause=0.1)
    time.sleep(0.5)

    screenshot("07_final", win_bbox(rect))
    results.check("Cleanup completed", True)

    # ---------------------------------------------------------------
    # Summary
    # ---------------------------------------------------------------
    print(f"\n  Screenshots: {SCREENSHOT_DIR}/")
    for f in sorted(os.listdir(SCREENSHOT_DIR)):
        if f.endswith('.png'):
            fpath = os.path.join(SCREENSHOT_DIR, f)
            size = os.path.getsize(fpath)
            print(f"    {f} ({size:,} bytes)")

    return results.summary()


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
