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
STEM_X, AMPLITUDE, TURNS = 46.0, 5.5, 2.0
LETTER_WIDTH, STRAND_WIDTH, RUNG_WIDTH = 4.8, 2.4, 1.3

G = "M36 41 C34 36 30 33.5 26 33.5 C20 33.5 16 41 16 54 C16 67 20 74.5 26 74.5 C31 74.5 36 71 36 65 L36 56 L27 56"
B = "M53 35 H59 C65 35 67.5 39 67.5 44 C67.5 49 64.5 53 59 53 H53 M53 53 H60 C66.5 53 69.5 58 69.5 63 C69.5 68 66.5 73 60 73 H53"
A = "M77 74.5 L85.5 33.5 L94 74.5 M80 62 H91"


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
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="8 26 94 56" role="img" aria-label="GBADN">\n'
        f'  <rect x="8" y="26" width="94" height="56" rx="8" fill="{GROUND}"/>\n'
        + svg_paths()
        + "\n</svg>\n"
    )


def write_favicon(path):
    path.write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="8 7 94 94">\n'
        f'  <rect x="8" y="7" width="94" height="94" rx="14" fill="{GROUND}"/>\n'
        + svg_paths()
        + "\n</svg>\n"
    )


def write_android_icon(path):
    """Adaptive icons mask to as little as a 66-unit circle, so the word is scaled to fit it."""
    scale = 0.69
    middle_x, middle_y = (16 + 94) / 2, 54
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
