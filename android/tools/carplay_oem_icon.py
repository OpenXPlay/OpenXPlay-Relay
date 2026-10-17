#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-17
"""Renders the CarPlay car-button (OEM) icon from the OpenXPlay launcher icon.

The iPhone shows this icon, with the OEM label, as the "back to the car" tile. Both relays send the
same 192-pixel opaque PNG: the Android app overrides DiPlay's packaged res/raw/ic_car_home.png, which
DiPlay sends unscaled up to that size, and the Linux relay embeds a copy in direlay-linux.

The launcher icon is adaptive: a 108 dp foreground over a solid background, of which launchers show
the central 72 dp. The tile shows that same viewport so it looks like the launcher icon.

Usage (from any directory; needs Pillow): python3 android/tools/carplay_oem_icon.py
"""

from pathlib import Path
import re

from PIL import Image

RELAY = Path(__file__).resolve().parents[2]
RES = RELAY / "android/app/src/main/res"
FOREGROUND = RES / "mipmap-xxxhdpi/ic_launcher_foreground.png"
BACKGROUND = RES / "values/ic_launcher_background.xml"
OUTPUTS = (
    RES / "raw/ic_car_home.png",
    RELAY / "rust/direlay-linux/assets/oem-icon.png",
)
# DiPlay's AirPlayIconSizing.MAX_PIXELS: the size of its packaged icon.
PIXELS = 192
# Adaptive icons: the visible viewport is 72 dp of the 108 dp layer.
VIEWPORT = 72 / 108


def background_colour() -> tuple[int, int, int]:
    xml = BACKGROUND.read_text(encoding="utf-8")
    match = re.search(r'name="ic_launcher_background">#([0-9A-Fa-f]{6})<', xml)
    if match is None:
        raise SystemExit(f"no ic_launcher_background colour in {BACKGROUND}")
    value = int(match.group(1), 16)
    return value >> 16, (value >> 8) & 0xFF, value & 0xFF


def render() -> Image.Image:
    foreground = Image.open(FOREGROUND).convert("RGBA")
    size = foreground.width
    if size != foreground.height:
        raise SystemExit(f"{FOREGROUND} is not square")
    # Composite at full resolution so the downscale never blends transparent fringe colours.
    canvas = Image.new("RGBA", foreground.size, background_colour() + (255,))
    canvas.alpha_composite(foreground)
    inset = round(size * (1 - VIEWPORT) / 2)
    viewport = canvas.crop((inset, inset, size - inset, size - inset)).convert("RGB")
    return viewport.resize((PIXELS, PIXELS), Image.Resampling.LANCZOS)


def main() -> None:
    icon = render()
    for output in OUTPUTS:
        output.parent.mkdir(parents=True, exist_ok=True)
        # Opaque RGB without metadata chunks, so the same inputs give the same bytes.
        icon.save(output, format="PNG", optimize=True)
        print(f"{output.relative_to(RELAY)}: {PIXELS}x{PIXELS}, {output.stat().st_size} bytes")


if __name__ == "__main__":
    main()
