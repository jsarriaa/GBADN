#!/usr/bin/env python3
"""Draws the genome-study figures from the tables written by gbadn-study.

Usage: plot-study.py <tables-folder> <figures-folder>
Needs the packages in scripts/requirements-study.txt.
"""

import csv
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt
import numpy as np
from matplotlib.collections import LineCollection
from matplotlib.colors import LinearSegmentedColormap
from matplotlib.patches import FancyBboxPatch, Patch
from pycirclize import Circos

SURFACE = "#fcfcfb"
INK = "#0b0b0b"
SECONDARY = "#52514e"
MUTED = "#898781"
GRID = "#e1e0d9"
AXIS = "#c3c2b7"
BLUE_RAMP = ["#cde2fb", "#86b6ef", "#3987e5", "#1c5cab", "#0d366b"]

CHROMOSOMES = {
    "chrHeader": "#6250d6",
    "chrCode": "#2a78d6",
    "chrGraphics": "#eb6834",
    "chrAudio": "#1baf7a",
    "chrData": "#eda100",
    "chrEmpty": "#898781",
}
BASES = {"A": "#2a78d6", "C": "#eb6834", "G": "#1baf7a", "T": "#eda100"}
WITH_CONTENT = ["chrCode", "chrGraphics", "chrAudio", "chrData"]

plt.rcParams.update(
    {
        "font.family": "sans-serif",
        "font.size": 10,
        "figure.facecolor": SURFACE,
        "axes.facecolor": SURFACE,
        "savefig.facecolor": SURFACE,
        "text.color": INK,
        "axes.labelcolor": SECONDARY,
        "axes.edgecolor": AXIS,
        "axes.spines.top": False,
        "axes.spines.right": False,
        "axes.titlesize": 11,
        "axes.titleweight": "bold",
        "axes.titlelocation": "left",
        "xtick.color": MUTED,
        "ytick.color": MUTED,
        "xtick.labelcolor": SECONDARY,
        "ytick.labelcolor": SECONDARY,
        "grid.color": GRID,
        "grid.linewidth": 0.7,
        "legend.frameon": False,
    }
)


def read(folder, name):
    with open(folder / f"{name}.tsv", newline="", encoding="utf-8") as file:
        return list(csv.DictReader(file, delimiter="\t"))


def megabases(bases):
    return bases / 1e6


def size_label(bases):
    return f"{bases / 1e6:.1f} Mbp" if bases >= 100_000 else f"{bases} bp"


def heading(figure, title, subtitle):
    figure.text(0.02, 0.975, title, fontsize=15, fontweight="bold", va="top")
    figure.text(0.02, 0.935, subtitle, fontsize=9.5, color=SECONDARY, va="top")


def save(figure, folder, name):
    figure.savefig(folder / f"{name}.png", dpi=160)
    plt.close(figure)


def karyotype(tables, folder, name):
    sizes = {row["chromosome"]: int(row["bases"]) for row in tables["chromosomes"]}
    ranked = sorted(sizes, key=sizes.get, reverse=True)
    tallest = sizes[ranked[0]]
    giemsa = LinearSegmentedColormap.from_list("giemsa", ["#f4f3ef", "#1b1b1a"])

    figure, axes = plt.subplots(figsize=(9.5, 7.2))
    figure.subplots_adjust(left=0.08, right=0.86, top=0.84, bottom=0.14)
    heading(
        figure,
        f"Karyotype of {name}",
        "Chromosomes ranked by size. Bands show GC per 100 kbp window; dark is AT-rich, as in Giemsa staining.",
    )
    width = 0.34
    for position, chromosome in enumerate(ranked):
        rows = [r for r in tables["windows"] if r["chromosome"] == chromosome]
        size = sizes[chromosome]
        # Too small to draw to scale, so it gets a visible minimum.
        height = max(size, tallest * 0.012)
        top = tallest
        box = FancyBboxPatch(
            (position - width / 2, top - height),
            width,
            height,
            boxstyle=f"round,pad=0,rounding_size={min(width / 2, height / (tallest / 7.5) / 2)}",
            mutation_aspect=tallest / 7.5,
            facecolor="none",
            edgecolor=SECONDARY,
            linewidth=1,
            zorder=3,
        )
        axes.add_patch(box)
        gc = np.array([[1 - float(r["gc"])] for r in rows])
        image = axes.imshow(
            gc,
            aspect="auto",
            cmap=giemsa,
            vmin=0.35,
            vmax=1.0,
            extent=(position - width / 2, position + width / 2, top - height, top),
            interpolation="nearest",
            zorder=2,
        )
        image.set_clip_path(box)
        axes.text(
            position,
            -tallest * 0.025,
            chromosome.removeprefix("chr"),
            ha="center",
            va="top",
            fontweight="bold",
        )
        axes.text(
            position,
            -tallest * 0.065,
            size_label(size),
            ha="center",
            va="top",
            color=SECONDARY,
            fontsize=9,
        )
        if chromosome in WITH_CONTENT and len(rows) > 1:
            lowest = min(rows, key=lambda r: float(r["entropy"]))
            middle = (int(lowest["start"]) + int(lowest["end"])) / 2
            axes.plot(
                position + width / 2 + 0.05,
                top - middle,
                marker="<",
                color="#e34948",
                markersize=7,
                zorder=4,
            )
    axes.set_xlim(-0.6, len(ranked) - 0.4)
    axes.set_ylim(-tallest * 0.02, tallest * 1.01)
    axes.axis("off")
    axes.plot([], [], marker="<", color="#e34948", linestyle="", label="Lowest-entropy window")
    axes.legend(loc="lower right", bbox_to_anchor=(1.0, 0.0))

    bar = figure.add_axes((0.89, 0.3, 0.018, 0.4))
    colorbar = figure.colorbar(image, cax=bar)
    colorbar.set_label("AT content")
    colorbar.outline.set_edgecolor(AXIS)
    figure.text(
        0.02,
        0.025,
        "Chromosome labels are best guesses from the content. chrHeader is drawn larger than scale.",
        fontsize=8.5,
        color=MUTED,
    )
    save(figure, folder, "karyotype")


def circos_plot(tables, folder, name):
    sizes = {row["chromosome"]: int(row["bases"]) for row in tables["chromosomes"]}
    sectors = {k: sizes[k] for k in WITH_CONTENT + ["chrEmpty"] if k in sizes}
    circos = Circos(sectors, space=2.5, start=8, end=352)
    ramp = LinearSegmentedColormap.from_list("entropy", BLUE_RAMP)

    # Exact repeats are tiny on this scale, so ribbons get a visible minimum width.
    ribbon = sum(sectors.values()) * 0.0016
    for sector in circos.sectors:
        rows = [r for r in tables["windows"] if r["chromosome"] == sector.name]
        middle = np.array([(int(r["start"]) + int(r["end"])) / 2 for r in rows])
        gc = np.array([float(r["gc"]) for r in rows])
        entropy = np.array([float(r["entropy"]) for r in rows])

        outer = sector.add_track((94, 98))
        outer.axis(fc=CHROMOSOMES[sector.name], ec="none")
        outer.xticks_by_interval(
            5_000_000,
            label_formatter=lambda value: f"{value / 1e6:.0f}",
            label_size=7,
            tick_length=1,
            label_orientation="horizontal",
        )
        sector.text(sector.name.removeprefix("chr"), r=113, size=11, weight="bold")

        track = sector.add_track((78, 92))
        track.axis(fc=SURFACE, ec=GRID, lw=0.6)
        track.fill_between(middle, gc, y2=0, vmin=0, vmax=1, color=CHROMOSOMES[sector.name], lw=0)

        strip = sector.add_track((70, 76))
        strip.axis(ec="none")
        strip.heatmap(entropy.reshape(1, -1), vmin=0, vmax=8, cmap=ramp)

    for row in tables["repeats"]:
        first, second = row["first"], row["second"]
        if first not in sectors or second not in sectors:
            continue
        a, b, length = int(row["first_start"]), int(row["second_start"]), int(row["bases"])
        span = max(length, ribbon)
        circos.link(
            (first, a, min(a + span, sectors[first])),
            (second, b, min(b + span, sectors[second])),
            color=CHROMOSOMES[first],
            alpha=0.45,
            r1=68,
            r2=68,
        )

    figure = circos.plotfig(figsize=(9.5, 10.4))
    figure.axes[0].set_position((0.08, 0.06, 0.84, 0.78))
    heading(
        figure,
        f"Circos plot of {name}",
        "Outside in: chromosomes (scale in Mbp), GC content, byte entropy, and exact repeats of at least 2 kbp.",
    )
    figure.text(
        0.02,
        0.025,
        "Repeat ribbons are widened to stay visible. chrHeader (768 bp) is left out. Labels are best guesses.",
        fontsize=8.5,
        color=MUTED,
    )
    save(figure, folder, "circos")


def assembly(tables, folder, name, summary):
    contigs = [(r["chromosome"], int(r["rom_offset"]) * 4, int(r["bases"])) for r in tables["contigs"]]
    total = sum(length for _, _, length in contigs)

    figure = plt.figure(figsize=(11, 7))
    grid = figure.add_gridspec(2, 2, height_ratios=[1, 4.2], left=0.07, right=0.97, top=0.82, bottom=0.13, hspace=0.42, wspace=0.22)
    heading(
        figure,
        f"Assembly report for {name}",
        f"{megabases(total):.1f} Mbp in {summary['contigs']} contigs · N50 {size_label(int(summary['n50']))} · L50 {summary['l50']} · rebuilt exactly, checked by MD5",
    )

    painting = figure.add_subplot(grid[0, :])
    for chromosome, color in CHROMOSOMES.items():
        spans = [(megabases(o), megabases(l)) for c, o, l in contigs if c == chromosome]
        painting.broken_barh(spans, (0, 1), facecolors=color, linewidth=0)
    painting.set_xlim(0, megabases(total))
    painting.set_yticks([])
    painting.spines["left"].set_visible(False)
    painting.set_xlabel("Position in the ROM (Mbp)")
    painting.set_title("Contig map")
    handles = [Patch(color=c, label=k.removeprefix("chr")) for k, c in CHROMOSOMES.items()]
    painting.legend(handles=handles, ncols=6, loc="lower right", bbox_to_anchor=(1, 1.02), fontsize=9, handlelength=1, columnspacing=1)

    lengths = np.sort([length for _, _, length in contigs])[::-1]
    covered = np.cumsum(lengths) / total * 100
    nx = figure.add_subplot(grid[1, 0])
    nx.step(np.r_[0, covered], np.r_[lengths[0], lengths] / 1e6, where="pre", color=BASES["A"], lw=2)
    nx.set_yscale("log")
    nx.set_xlim(0, 100)
    nx.set_xlabel("x (%)")
    nx.set_ylabel("Nx (Mbp, log scale)")
    nx.set_title("Nx curve")
    nx.grid(True, axis="y")
    n50 = int(summary["n50"]) / 1e6
    nx.plot([50], [n50], marker="o", markersize=8, color=BASES["A"], markeredgecolor=SURFACE, markeredgewidth=2)
    nx.annotate(f"N50 = {n50:.1f} Mbp", (50, n50), xytext=(10, 8), textcoords="offset points")

    cumulative = figure.add_subplot(grid[1, 1])
    for chromosome in WITH_CONTENT + ["chrEmpty"]:
        sizes = np.sort([l for c, _, l in contigs if c == chromosome])[::-1]
        if len(sizes) == 0:
            continue
        ranks = np.arange(0, len(sizes) + 1)
        totals = np.r_[0, np.cumsum(sizes)] / 1e6
        cumulative.plot(ranks, totals, color=CHROMOSOMES[chromosome], lw=2)
        cumulative.annotate(
            chromosome.removeprefix("chr"),
            (ranks[-1], totals[-1]),
            xytext=(5, 0),
            textcoords="offset points",
            va="center",
            fontsize=9,
            color=INK,
        )
    cumulative.set_xscale("symlog", linthresh=1)
    cumulative.set_xlim(0, None)
    cumulative.margins(x=0.14)
    cumulative.set_xlabel("Contigs, longest first (log scale)")
    cumulative.set_ylabel("Cumulative length (Mbp)")
    cumulative.set_title("Contiguity by chromosome")
    cumulative.grid(True, axis="y")
    figure.text(0.02, 0.02, "A contig here is one consecutive run of the ROM with a single best-guess label.", fontsize=8.5, color=MUTED)
    save(figure, folder, "assembly")


def dotplot(tables, folder, name):
    sizes = {row["chromosome"]: int(row["bases"]) for row in tables["chromosomes"]}
    starts, cursor = {}, 0
    for chromosome in WITH_CONTENT:
        starts[chromosome] = cursor
        cursor += sizes.get(chromosome, 0)

    segments = []
    for row in tables["repeats"]:
        if row["first"] not in starts or row["second"] not in starts:
            continue
        x = (starts[row["first"]] + int(row["first_start"])) / 1e6
        y = (starts[row["second"]] + int(row["second_start"])) / 1e6
        length = int(row["bases"]) / 1e6
        segments.append(((x, y), (x + length, y + length)))
        segments.append(((y, x), (y + length, x + length)))

    figure, axes = plt.subplots(figsize=(8.8, 9.2))
    figure.subplots_adjust(left=0.1, right=0.97, top=0.83, bottom=0.09)
    heading(
        figure,
        f"Self dot plot of {name}",
        "Each mark is a stretch found twice, at least 2 kbp long. The main diagonal is the genome against itself.",
    )
    end = cursor / 1e6
    axes.plot([0, end], [0, end], color=AXIS, lw=1, zorder=1)
    axes.add_collection(LineCollection(segments, colors=BLUE_RAMP[3], linewidths=1.8, capstyle="round", zorder=3))
    ticks = []
    for chromosome in WITH_CONTENT:
        start = starts[chromosome] / 1e6
        size = sizes.get(chromosome, 0) / 1e6
        if start > 0:
            axes.axvline(start, color=GRID, lw=0.8, zorder=0)
            axes.axhline(start, color=GRID, lw=0.8, zorder=0)
        ticks.append(start + size / 2)
    labels = [c.removeprefix("chr") for c in WITH_CONTENT]
    axes.set_xticks(ticks, labels)
    axes.set_yticks(ticks, labels, rotation=90, va="center")
    axes.tick_params(length=0)
    axes.set_xlim(0, end)
    axes.set_ylim(0, end)
    axes.set_aspect("equal")
    figure.text(0.02, 0.025, "Empty space is left out. Marks are drawn thicker than scale. Labels are best guesses.", fontsize=8.5, color=MUTED)
    save(figure, folder, "dotplot")


def quality(tables, folder, name, summary):
    figure = plt.figure(figsize=(12, 6.4))
    outer = figure.add_gridspec(1, 3, left=0.06, right=0.98, top=0.8, bottom=0.2, wspace=0.26)
    heading(
        figure,
        f"Sequence quality report for {name}",
        "Measured on the decoded ROM at 4 bases per byte. The zstd-compressed text in the FASTA file itself looks random.",
    )

    composition = figure.add_subplot(outer[0, 0])
    rows = [r for r in tables["chromosomes"] if r["chromosome"] in WITH_CONTENT + ["chrEmpty"]]
    positions = np.arange(len(rows))[::-1]
    left = np.zeros(len(rows))
    totals = np.array([int(r["bases"]) for r in rows])
    for base, color in BASES.items():
        share = np.array([int(r[base]) for r in rows]) / totals * 100
        composition.barh(positions, share, left=left, height=0.56, color=color, edgecolor=SURFACE, linewidth=1.5, label=base)
        left += share
    composition.set_yticks(positions, [r["chromosome"].removeprefix("chr") for r in rows])
    composition.set_xlim(0, 100)
    composition.set_xlabel("Share of bases (%)")
    composition.set_title("Base composition")
    composition.legend(ncols=4, loc="upper center", bbox_to_anchor=(0.5, -0.17), handlelength=1, columnspacing=1, fontsize=9)
    composition.tick_params(axis="y", length=0)

    ridge = outer[0, 1].subgridspec(4, 1, hspace=0.25)
    bins = np.linspace(0, 1, 41)
    for index, chromosome in enumerate(WITH_CONTENT):
        axes = figure.add_subplot(ridge[index, 0])
        values = [float(r["gc"]) for r in tables["windows"] if r["chromosome"] == chromosome]
        axes.hist(values, bins=bins, color=CHROMOSOMES[chromosome], rwidth=0.86)
        axes.set_xlim(0, 1)
        axes.set_yticks([])
        axes.spines["left"].set_visible(False)
        axes.text(0.99, 0.82, chromosome.removeprefix("chr"), transform=axes.transAxes, ha="right", fontsize=9)
        if index == 0:
            axes.set_title("GC content per 100 kbp window")
        if index < len(WITH_CONTENT) - 1:
            axes.set_xticklabels([])
        else:
            axes.set_xticks([0, 0.25, 0.5, 0.75, 1], ["0%", "25%", "50%", "75%", "100%"])
            axes.set_xlabel("GC")

    spectrum = figure.add_subplot(outer[0, 2])
    multiplicity = np.array([int(r["multiplicity"]) for r in tables["kmers"]])
    distinct = np.array([int(r["distinct_kmers"]) for r in tables["kmers"]])
    spectrum.loglog(multiplicity, distinct, color=BASES["A"], lw=0, marker="o", markersize=3)
    spectrum.set_xlabel("Times a k-mer occurs")
    spectrum.set_ylabel("Distinct k-mers")
    spectrum.set_title(f"{summary['k']}-mer spectrum")
    spectrum.grid(True, which="major")
    figure.text(0.02, 0.02, "Empty space is left out of the k-mer spectrum. Chromosome labels are best guesses.", fontsize=8.5, color=MUTED)
    save(figure, folder, "quality")


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    source, output = Path(sys.argv[1]), Path(sys.argv[2])
    output.mkdir(parents=True, exist_ok=True)
    tables = {
        name: read(source, name)
        for name in ["chromosomes", "contigs", "windows", "repeats", "kmers"]
    }
    summary = {row["field"]: row["value"] for row in read(source, "summary")}
    name = f"{summary['title']} ({summary['code']})"
    karyotype(tables, output, name)
    circos_plot(tables, output, name)
    assembly(tables, output, name, summary)
    dotplot(tables, output, name)
    quality(tables, output, name, summary)


if __name__ == "__main__":
    main()
