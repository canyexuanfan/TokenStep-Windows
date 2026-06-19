#!/usr/bin/env python3
from __future__ import annotations

import math
import shutil
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


ROOT = Path(__file__).resolve().parent
ASSETS = ROOT / "assets"
ICONSET = ASSETS / "TokenStepIcon.iconset"
PNG = ASSETS / "TokenStepIcon.png"
ICNS = ASSETS / "TokenStepIcon.icns"

SIZE = 1024
SCALE = 3
W = SIZE * SCALE


def sx(value: float) -> int:
    return round(value * SCALE)


def hex_to_rgb(value: str) -> tuple[int, int, int]:
    value = value.lstrip("#")
    return tuple(int(value[i : i + 2], 16) for i in (0, 2, 4))


def mix(a: str, b: str, t: float) -> tuple[int, int, int]:
    ar, ag, ab = hex_to_rgb(a)
    br, bg, bb = hex_to_rgb(b)
    return (
        round(ar + (br - ar) * t),
        round(ag + (bg - ag) * t),
        round(ab + (bb - ab) * t),
    )


def draw_gradient_round_rect(draw: ImageDraw.ImageDraw, rect, radius: int) -> None:
    x0, y0, x1, y1 = rect
    height = y1 - y0
    for offset in range(height):
        t = offset / max(1, height - 1)
        shade = mix("#ffffff", "#f3f7f4", t)
        draw.rounded_rectangle((x0, y0 + offset, x1, y0 + offset + 1), radius=radius, fill=shade)


def arc_points(cx: int, cy: int, radius: int, start_deg: float, end_deg: float, count: int) -> list[tuple[int, int]]:
    points = []
    for index in range(count + 1):
        angle = math.radians(start_deg + (end_deg - start_deg) * index / count)
        points.append((round(cx + math.cos(angle) * radius), round(cy + math.sin(angle) * radius)))
    return points


def draw_arc_with_caps(
    draw: ImageDraw.ImageDraw,
    cx: int,
    cy: int,
    radius: int,
    start_deg: float,
    end_deg: float,
    width: int,
    start_color: str,
    end_color: str,
) -> None:
    points = arc_points(cx, cy, radius, start_deg, end_deg, 180)
    for index in range(len(points) - 1):
        t = index / max(1, len(points) - 2)
        draw.line([points[index], points[index + 1]], fill=mix(start_color, end_color, t), width=width)
    cap_radius = width // 2
    draw.ellipse(
        (
            points[0][0] - cap_radius,
            points[0][1] - cap_radius,
            points[0][0] + cap_radius,
            points[0][1] + cap_radius,
        ),
        fill=hex_to_rgb(start_color),
    )
    draw.ellipse(
        (
            points[-1][0] - cap_radius,
            points[-1][1] - cap_radius,
            points[-1][0] + cap_radius,
            points[-1][1] + cap_radius,
        ),
        fill=hex_to_rgb(end_color),
    )


def draw_solid_arc_with_caps(
    draw: ImageDraw.ImageDraw,
    cx: int,
    cy: int,
    radius: int,
    start_deg: float,
    end_deg: float,
    width: int,
    fill: str,
    end_fill: str | None = None,
) -> None:
    bbox = (cx - radius, cy - radius, cx + radius, cy + radius)
    draw.arc(bbox, start=start_deg, end=end_deg, fill=hex_to_rgb(fill), width=width)
    cap_radius = width // 2
    start = arc_points(cx, cy, radius, start_deg, start_deg, 1)[0]
    end = arc_points(cx, cy, radius, end_deg, end_deg, 1)[0]
    draw.ellipse(
        (start[0] - cap_radius, start[1] - cap_radius, start[0] + cap_radius, start[1] + cap_radius),
        fill=hex_to_rgb(fill),
    )
    draw.ellipse(
        (end[0] - cap_radius, end[1] - cap_radius, end[0] + cap_radius, end[1] + cap_radius),
        fill=hex_to_rgb(end_fill or fill),
    )


def draw_token(draw: ImageDraw.ImageDraw, cx: int, cy: int, size: int, fill: str, shadow: Image.Image) -> None:
    radius = round(size * 0.24)
    rect = (cx - size // 2, cy - size // 2, cx + size // 2, cy + size // 2)
    shadow_draw = ImageDraw.Draw(shadow)
    shadow_draw.rounded_rectangle(
        (rect[0] + sx(6), rect[1] + sx(8), rect[2] + sx(6), rect[3] + sx(8)),
        radius=radius,
        fill=(18, 24, 38, 34),
    )
    draw.rounded_rectangle(rect, radius=radius, fill=hex_to_rgb(fill))
    inset = round(size * 0.16)
    draw.rounded_rectangle(
        (rect[0] + inset, rect[1] + inset, rect[2] - inset, rect[1] + inset + max(2, size // 12)),
        radius=max(1, size // 20),
        fill=(255, 255, 255, 70),
    )


def render_base_icon() -> Image.Image:
    image = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow)

    base_rect = (sx(76), sx(72), sx(948), sx(948))
    shadow_draw.rounded_rectangle(
        (base_rect[0], base_rect[1] + sx(24), base_rect[2], base_rect[3] + sx(24)),
        radius=sx(208),
        fill=(18, 24, 38, 44),
    )
    image.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(sx(22))))

    draw = ImageDraw.Draw(image)
    draw_gradient_round_rect(draw, base_rect, sx(206))
    draw.rounded_rectangle(base_rect, radius=sx(206), outline=(255, 255, 255, 205), width=sx(5))
    draw.rounded_rectangle(
        (base_rect[0] + sx(12), base_rect[1] + sx(12), base_rect[2] - sx(12), base_rect[3] - sx(12)),
        radius=sx(194),
        outline=(219, 227, 223, 120),
        width=sx(2),
    )

    cx = cy = sx(512)
    radius = sx(286)
    width = sx(82)

    ring_shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ring_shadow_draw = ImageDraw.Draw(ring_shadow)
    ring_shadow_draw.ellipse(
        (cx - radius, cy - radius + sx(7), cx + radius, cy + radius + sx(7)),
        outline=(18, 24, 38, 42),
        width=width,
    )
    image.alpha_composite(ring_shadow.filter(ImageFilter.GaussianBlur(sx(8))).point(lambda p: p // 6))

    draw.ellipse((cx - radius, cy - radius, cx + radius, cy + radius), outline=hex_to_rgb("#e9edf2"), width=width)
    draw_solid_arc_with_caps(draw, cx, cy, radius, -90, 158, width, "#2da44e", "#216e39")

    inner_highlight = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    highlight_draw = ImageDraw.Draw(inner_highlight)
    draw_solid_arc_with_caps(highlight_draw, cx, cy - sx(5), radius - sx(24), -82, 145, sx(5), "#d8f3dc")
    image.alpha_composite(inner_highlight.filter(ImageFilter.GaussianBlur(sx(1))).point(lambda p: min(255, p // 2)))

    token_shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    token_size = sx(68)
    gap = sx(54)
    token_positions = [
        (cx, cy - gap),
        (cx - gap, cy),
        (cx, cy),
        (cx + gap, cy),
        (cx, cy + gap),
    ]
    colors = ["#9be9a8", "#40c463", "#2da44e", "#216e39", "#30a14e"]
    for (tx, ty), fill in zip(token_positions, colors):
        draw_token(draw, tx, ty, token_size, fill, token_shadow)
    image.alpha_composite(token_shadow.filter(ImageFilter.GaussianBlur(sx(5))))

    # Redraw tokens after the shadow layer so their edges stay crisp.
    draw = ImageDraw.Draw(image)
    for (tx, ty), fill in zip(token_positions, colors):
        radius_token = round(token_size * 0.24)
        rect = (tx - token_size // 2, ty - token_size // 2, tx + token_size // 2, ty + token_size // 2)
        draw.rounded_rectangle(rect, radius=radius_token, fill=hex_to_rgb(fill))
        draw.rounded_rectangle(rect, radius=radius_token, outline=(255, 255, 255, 72), width=sx(2))

    return image.resize((SIZE, SIZE), Image.Resampling.LANCZOS)


def save_iconset(icon: Image.Image) -> None:
    if ICONSET.exists():
        shutil.rmtree(ICONSET)
    ICONSET.mkdir(parents=True, exist_ok=True)
    sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]
    for size, name in sizes:
        icon.resize((size, size), Image.Resampling.LANCZOS).save(ICONSET / name)


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    icon = render_base_icon()
    icon.save(PNG)
    save_iconset(icon)
    if ICNS.exists():
        ICNS.unlink()
    subprocess.run(["iconutil", "-c", "icns", str(ICONSET), "-o", str(ICNS)], check=True)
    print(PNG)
    print(ICNS)


if __name__ == "__main__":
    main()
