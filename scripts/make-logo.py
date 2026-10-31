#!/usr/bin/env python3
"""Draws the GBADN logo: the letters GBA, with the stem of the B as a DNA chain.

Writes the SVG used by the website and README, and the Android launcher icon.
"""
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GROUND = "#16181C"
LETTER = "#E7E9EC"
STRAND_A = "#5BBE6E"
STRAND_B = "#5AA9E6"
RUNG = "#A3A9B2"

TOP, BOTTOM = 33.0, 75.0
STEM_X, AMPLITUDE, TURNS = 50.0, 6.0, 2.0
LETTER_WIDTH, STRAND_WIDTH, RUNG_WIDTH = 6.5, 2.8, 1.6

G = "M41 41 C39 36 35 33.5 31 33.5 C25 33.5 21 41 21 54 C21 67 25 74.5 31 74.5 C36 74.5 41 71 41 65 L41 56 L32 56"
B = "M56 36 H60 C66 36 68.5 40 68.5 44.5 C68.5 49 65.5 53 60 53 H56 M56 53 H61 C67.5 53 70.5 58 70.5 63 C70.5 68 67.5 72 61 72 H56"
A = "M75 74.5 L83 33.5 L91 74.5 M77.5 62 H88.5"


def strand(phase):
    points = []
    for step in range(61):
        y = TOP + (BOTTOM - TOP) * step / 60
        angle = 2 * math.pi * TURNS * step / 60 + phase
        points.append((STEM_X + AMPLITUDE * math.sin(angle), y))
    return "M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in points)


def rungs():
    """Rungs sit where the strands are furthest apart, like base pairs."""
    lines = []
    for step in range(int(TURNS * 8)):
        fraction = (step + 0.5) / (TURNS * 8)
        angle = 2 * math.pi * TURNS * fraction
        half = AMPLITUDE * math.sin(angle)
        if abs(half) > 2.2:
            y = TOP + (BOTTOM - TOP) * fraction
            lines.append(f"M{STEM_X - half + (0.9 if half > 0 else -0.9):.2f} {y:.2f} H{STEM_X + half - (0.9 if half > 0 else -0.9):.2f}")
    return " ".join(lines)


PATHS = [
    (rungs(), RUNG, RUNG_WIDTH),
    (strand(math.pi), STRAND_B, STRAND_WIDTH),
    (strand(0.0), STRAND_A, STRAND_WIDTH),
    (G, LETTER, LETTER_WIDTH),
    (B, LETTER, LETTER_WIDTH),
    (A, LETTER, LETTER_WIDTH),
]


def svg_paths():
    return "\n".join(
        f'  <path d="{d}" fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/>'
        for d, color, width in PATHS
    )


def write_svg(path):
    path.write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="15 27 82 54" role="img" aria-label="GBADN">\n'
        + svg_paths()
        + "\n</svg>\n"
    )


def write_favicon(path):
    path.write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="10 -2 88 112">\n'
        f'  <rect x="10" y="-2" width="88" height="112" rx="14" fill="{GROUND}"/>\n'
        + svg_paths()
        + "\n</svg>\n"
    )


def write_android_icon(path):
    """Adaptive icons mask to as little as a 66-unit circle, so the word is scaled to fit it."""
    scale = 0.74
    middle_x, middle_y = (21 + 91) / 2, 54
    shapes = "\n".join(
        f'        <path android:pathData="{d}" android:strokeColor="{color}" android:strokeWidth="{width}" '
        'android:strokeLineCap="round" android:strokeLineJoin="round"/>'
        for d, color, width in PATHS
    )
    path.write_text(
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
        '    android:width="108dp" android:height="108dp"\n'
        '    android:viewportWidth="108" android:viewportHeight="108">\n'
        f'    <group android:scaleX="{scale}" android:scaleY="{scale}" '
        f'android:translateX="{54 - middle_x * scale:.2f}" android:translateY="{54 - middle_y * scale:.2f}">\n'
        + shapes
        + "\n    </group>\n</vector>\n"
    )


write_svg(ROOT / "assets/logo.svg")
write_favicon(ROOT / "web/favicon.svg")
write_android_icon(ROOT / "android/res/drawable/ic_launcher_foreground.xml")
