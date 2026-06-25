from __future__ import annotations

import argparse
import functools
import http.server
import json
import shutil
import subprocess
import sys
import threading
from pathlib import Path
from typing import Any

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))


class _QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: Any) -> None:
        return


def verify_viewer_visual(
    viewer_dir: str | Path,
    *,
    screenshot: str | Path,
    timeout_ms: int = 30000,
) -> dict[str, Any]:
    npx = shutil.which("npx")
    if npx is None:
        return {"ok": False, "reason": "npx_missing"}
    root = Path(viewer_dir).resolve()
    if not (root / "index.html").exists():
        return {"ok": False, "reason": "viewer_index_missing", "viewer_dir": str(root)}
    screenshot_path = Path(screenshot).resolve()
    screenshot_path.parent.mkdir(parents=True, exist_ok=True)
    handler = functools.partial(_QuietHandler, directory=str(root))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        url = f"http://127.0.0.1:{server.server_port}/index.html"
        return _run_playwright(
            npx,
            url=url,
            screenshot=screenshot_path,
            timeout_ms=timeout_ms,
        )
    finally:
        server.shutdown()
        server.server_close()


def _run_playwright(
    npx: str,
    *,
    url: str,
    screenshot: Path,
    timeout_ms: int,
) -> dict[str, Any]:
    result = subprocess.run(
        [
            npx,
            "--yes",
            "playwright",
            "screenshot",
            "--browser",
            "chromium",
            "--viewport-size",
            "1280,800",
            "--wait-for-selector",
            "#scene",
            "--wait-for-timeout",
            "1500",
            "--timeout",
            str(timeout_ms),
            url,
            str(screenshot),
        ],
        check=False,
        capture_output=True,
        text=True,
        timeout=max(10, timeout_ms // 1000 + 20),
    )
    if result.returncode != 0:
        return {
            "ok": False,
            "reason": "playwright_failed",
            "returncode": result.returncode,
            "stdout": result.stdout[-2000:],
            "stderr": result.stderr[-2000:],
        }
    canvas = _sample_canvas_region(screenshot)
    return {
        "ok": bool(canvas["ok"]),
        "reason": "" if canvas["ok"] else canvas["reason"],
        "screenshot": str(screenshot),
        "canvas": canvas,
        "stdout": result.stdout[-2000:],
        "stderr": result.stderr[-2000:],
    }


def _sample_canvas_region(screenshot: Path) -> dict[str, Any]:
    try:
        from PIL import Image
    except Exception as exc:
        return {"ok": False, "reason": f"pillow_unavailable: {exc}"}
    if not screenshot.exists():
        return {"ok": False, "reason": "screenshot_missing"}
    image = Image.open(screenshot).convert("RGBA")
    width, height = image.size
    crop_top = int(height * 0.16)
    pixels = np.asarray(image.crop((0, crop_top, width, height)), dtype=np.uint8)
    flat_pixels = pixels.reshape(-1, 4)
    stride = max(1, len(flat_pixels) // 20000)
    alpha_samples = 0
    nonblank_samples = 0
    for index, (red, green, blue, alpha) in enumerate(flat_pixels):
        if index % stride:
            continue
        if alpha > 0:
            alpha_samples += 1
        if alpha > 0 and (
            abs(red - 17) > 3 or abs(green - 17) > 3 or abs(blue - 17) > 3
        ):
            nonblank_samples += 1
    return {
        "ok": nonblank_samples > 10,
        "reason": "" if nonblank_samples > 10 else "blank_canvas_region",
        "width": width,
        "height": height - crop_top,
        "crop_top": crop_top,
        "alpha_samples": alpha_samples,
        "nonblank_samples": nonblank_samples,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Playwright viewer visual QA.")
    parser.add_argument("viewer_dir", help="exported viewer directory")
    parser.add_argument(
        "--screenshot",
        default="out/viewer_visual_qa.png",
        help="screenshot output path",
    )
    parser.add_argument("--timeout-ms", default=30000, type=int)
    args = parser.parse_args()
    payload = verify_viewer_visual(
        args.viewer_dir,
        screenshot=args.screenshot,
        timeout_ms=int(args.timeout_ms),
    )
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0 if payload.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
