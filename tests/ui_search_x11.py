#!/usr/bin/env python3
"""Drive the native FastFind X11 UI with real Python-generated clicks and keys."""

from __future__ import annotations

import argparse
import ctypes
import os
import re
import subprocess
import sys
import tempfile
import time
import types
from pathlib import Path

# PyAutoGUI imports its optional MouseInfo/Tk helper eagerly on Linux. The UI
# test does not use that helper, and the host intentionally has no Tk package.
mouseinfo_stub = types.ModuleType("mouseinfo")
mouseinfo_stub.MouseInfoWindow = object
sys.modules.setdefault("mouseinfo", mouseinfo_stub)

import pyautogui


class X11Driver:
    def __init__(self) -> None:
        pyautogui.PAUSE = 0.15
        pyautogui.FAILSAFE = True
        self.x11 = ctypes.cdll.LoadLibrary("libX11.so.6")
        self.x11.XOpenDisplay.restype = ctypes.c_void_p
        self.display = self.x11.XOpenDisplay(None)
        if not self.display:
            raise RuntimeError("cannot open X display")

    def close(self) -> None:
        if self.display:
            self.x11.XCloseDisplay(ctypes.c_void_p(self.display))
            self.display = None

    def raise_window(self, window_id: int) -> None:
        display = ctypes.c_void_p(self.display)
        self.x11.XRaiseWindow(display, ctypes.c_ulong(window_id))
        self.x11.XFlush(display)
        time.sleep(0.2)

    def click(self, x: int, y: int) -> None:
        pyautogui.moveTo(x, y, duration=0.15)
        pyautogui.click()

    def hotkey(self, *names: str) -> None:
        aliases = {"Control_L": "ctrl", "Shift_L": "shift"}
        pyautogui.hotkey(*(aliases.get(name, name) for name in names))

    def type_text(self, text: str) -> None:
        if not (text.isascii() and text.isalnum()):
            raise ValueError("test driver currently types ASCII letters and digits only")
        pyautogui.write(text, interval=0.04)
        time.sleep(0.5)


def wait_for_window(process: subprocess.Popen[str], timeout: float = 10.0) -> int:
    deadline = time.monotonic() + timeout
    pattern = re.compile(r'^\s*(0x[0-9a-f]+).*"FastFind"', re.MULTILINE)
    while time.monotonic() < deadline:
        if process.poll() is not None:
            output = process.stdout.read() if process.stdout else ""
            raise RuntimeError(f"FastFind exited before creating a window: {output}")
        tree = subprocess.run(
            ["xwininfo", "-root", "-tree"],
            check=True,
            text=True,
            capture_output=True,
        ).stdout
        match = pattern.search(tree)
        if match:
            return int(match.group(1), 16)
        time.sleep(0.1)
    raise TimeoutError("FastFind X11 window did not appear")


def window_geometry(window_id: int) -> tuple[int, int, int, int]:
    output = subprocess.run(
        ["xwininfo", "-id", hex(window_id)],
        check=True,
        text=True,
        capture_output=True,
    ).stdout

    def value(label: str) -> int:
        match = re.search(rf"{re.escape(label)}:\s+(-?\d+)", output)
        if not match:
            raise RuntimeError(f"missing {label} in xwininfo output")
        return int(match.group(1))

    return (
        value("Absolute upper-left X"),
        value("Absolute upper-left Y"),
        value("Width"),
        value("Height"),
    )


def capture_and_ocr(destination: Path) -> str:
    subprocess.run(
        ["gnome-screenshot", "--window", f"--file={destination}"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    return subprocess.run(
        ["tesseract", str(destination), "stdout"],
        check=True,
        text=True,
        capture_output=True,
    ).stdout


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--database", type=Path, required=True)
    parser.add_argument("--query", required=True)
    parser.add_argument("--expected-path", type=Path, required=True)
    parser.add_argument(
        "--screenshot",
        type=Path,
        default=Path(tempfile.gettempdir()) / "fastfind-click-search.png",
    )
    args = parser.parse_args()

    process = subprocess.Popen(
        [
            str(args.binary),
            "--database",
            str(args.database),
            "--skip-feature-matrix",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env=os.environ.copy(),
    )
    driver = X11Driver()
    try:
        window_id = wait_for_window(process)
        x, y, width, height = window_geometry(window_id)
        driver.raise_window(window_id)
        # X11 maps the native window before egui completes its first dialog frame.
        time.sleep(1.0)

        # Activate through the native title bar; click-to-focus window managers may
        # consume the first click instead of forwarding it to the application.
        driver.click(x + width // 2, max(4, y - 14))
        time.sleep(0.3)

        # Click the search edit, then exercise FastFind's Ctrl+F focus command to
        # make focus deterministic across click-to-focus window managers.
        driver.click(x + int(width * 0.42), y + int(height * 0.06))
        driver.hotkey("Control_L", "f")
        driver.hotkey("Control_L", "a")
        driver.type_text(args.query)
        time.sleep(1.0)

        driver.raise_window(window_id)
        ocr = capture_and_ocr(args.screenshot)
        normalized_ocr = " ".join(ocr.lower().split())
        expected_name = args.expected_path.name.lower()
        if args.query.lower() not in normalized_ocr:
            raise AssertionError(f"typed query is absent from UI OCR: {ocr}")
        if expected_name not in normalized_ocr:
            raise AssertionError(f"expected result is absent from UI OCR: {ocr}")
        if not re.search(r"\b1 indexed matches\b", normalized_ocr):
            raise AssertionError(f"search did not narrow the indexed matches to one: {ocr}")

        print(f"PASS query={args.query!r}")
        print(f"PASS visible_result={args.expected_path.name!r}")
        print("PASS indexed_matches=1")
        print(f"screenshot={args.screenshot}")
        return 0
    finally:
        driver.close()
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise
