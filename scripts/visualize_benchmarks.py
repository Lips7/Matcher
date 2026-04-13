#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["plotly"]
# ///
"""Interactive benchmark visualization dashboard.

Reads aggregate.json (or run directories) produced by run_benchmarks.py and
generates an interactive HTML dashboard using Plotly.

Usage:
    uv run scripts/visualize_benchmarks.py <run_dir>                          # single run
    uv run scripts/visualize_benchmarks.py <baseline_dir> <candidate_dir>     # comparison
    uv run scripts/visualize_benchmarks.py <path> --open                      # auto-open browser
"""

from __future__ import annotations

import argparse
import dataclasses
import enum
import pathlib
import webbrowser
from collections import defaultdict

import plotly.graph_objects as go
from plotly.subplots import make_subplots

from bench_utils import (
    METRIC_CHOICES,
    AggregateFile,
    AggregateRow,
    format_duration,
    load_aggregate_input,
)

# =============================================================================
# Constants
# =============================================================================

SERIES_PALETTE = [
    "#4C78A8", "#E45756", "#54A24B", "#B279A2",
    "#F58518", "#72B7B2", "#EECA3B", "#FF9DA6",
]

LAYOUT_DEFAULTS = dict(
    template="plotly_white",
    font=dict(family="Inter, -apple-system, Segoe UI, sans-serif", size=13),
    paper_bgcolor="white",
    plot_bgcolor="#fafafa",
    hoverlabel=dict(font_size=12, bgcolor="white"),
    margin=dict(t=80, b=60, l=70, r=30),
)

PAGE_STYLE = """\
body {
  background: #f5f5f5; color: #222;
  font-family: 'Inter', -apple-system, 'Segoe UI', sans-serif;
  margin: 0; padding: 24px 20px;
}
h1 { text-align: center; color: #222; margin-bottom: 2px; font-weight: 700; }
.subtitle { text-align: center; color: #777; font-size: 14px; margin-top: 4px; }
.section { max-width: 1400px; margin: 28px auto; }
.meta-row { display: flex; gap: 48px; justify-content: center; flex-wrap: wrap; }
"""


# =============================================================================
# Data classification
# =============================================================================

class GroupKind(enum.Enum):
    SCALING = "scaling"
    BAR = "bar"


@dataclasses.dataclass
class ChartGroup:
    title: str
    kind: GroupKind
    rows: dict[str, AggregateRow]


def _pretty_title(raw: str) -> str:
    return raw.replace("_", " ").title()


def _split_key(key: str) -> list[str]:
    return [p.strip() for p in key.split("/")]


def _has_numeric(parts: list[str]) -> int | None:
    """Return index of the first purely-numeric segment, or None."""
    for i, p in enumerate(parts):
        if p.isdigit():
            return i
    return None


def classify_groups(agg: AggregateFile) -> list[ChartGroup]:
    buckets: dict[str, dict[str, AggregateRow]] = defaultdict(dict)
    for key, row in agg.rows.items():
        top = _split_key(key)[0]
        buckets[top][key] = row

    groups: list[ChartGroup] = []
    for top_key in sorted(buckets):
        rows = buckets[top_key]
        sample_parts = _split_key(next(iter(rows)))
        tail = sample_parts[1:]
        kind = GroupKind.SCALING if _has_numeric(tail) is not None else GroupKind.BAR
        groups.append(ChartGroup(title=_pretty_title(top_key), kind=kind, rows=rows))
    return groups


# =============================================================================
# Row decomposition helpers
# =============================================================================

@dataclasses.dataclass
class ScalingPoint:
    series: str
    size: int
    value_s: float
    row: AggregateRow


@dataclasses.dataclass
class BarPoint:
    group: str
    label: str
    value_s: float
    row: AggregateRow


def decompose_scaling(group: ChartGroup) -> tuple[list[str], list[int], dict[str, list[ScalingPoint]]]:
    """Extract series, sizes, and per-series points from a SCALING group."""
    all_series: set[str] = set()
    all_sizes: set[int] = set()
    by_series: dict[str, list[ScalingPoint]] = defaultdict(list)

    for key, row in group.rows.items():
        parts = _split_key(key)
        tail = parts[1:]
        num_idx = _has_numeric(tail)
        if num_idx is None:
            continue
        size = int(tail[num_idx])
        non_numeric = [p for i, p in enumerate(tail) if i != num_idx]
        series = " / ".join(non_numeric) if non_numeric else parts[0]

        pt = ScalingPoint(series=series, size=size, value_s=row.value_s, row=row)
        by_series[series].append(pt)
        all_series.add(series)
        all_sizes.add(size)

    for pts in by_series.values():
        pts.sort(key=lambda p: p.size)

    ordered = sorted(all_series)
    return ordered, sorted(all_sizes), {s: by_series[s] for s in ordered}


def decompose_bars(group: ChartGroup) -> tuple[list[str], list[str], dict[str, dict[str, BarPoint]]]:
    """Extract groups, labels, and grid of points from a BAR group.

    Returns (group_names, label_names, grid[group][label] -> BarPoint).
    For 2-part keys: single group (the top-level), labels are the second part.
    For 3-part keys: groups are the second part, labels are the third part.
    """
    grid: dict[str, dict[str, BarPoint]] = defaultdict(dict)

    for key, row in group.rows.items():
        parts = _split_key(key)
        if len(parts) == 2:
            grp, label = "", parts[1]
        elif len(parts) >= 3:
            grp, label = parts[1], " / ".join(parts[2:])
        else:
            continue
        grid[grp][label] = BarPoint(group=grp, label=label, value_s=row.value_s, row=row)

    groups_ordered = sorted(grid)
    all_labels: dict[str, None] = {}
    for grp in groups_ordered:
        for label in sorted(grid[grp]):
            all_labels[label] = None
    labels_ordered = list(all_labels)

    return groups_ordered, labels_ordered, grid


# =============================================================================
# Chart builders
# =============================================================================

def _color_for(idx: int) -> str:
    return SERIES_PALETTE[idx % len(SERIES_PALETTE)]


def build_scaling_fig(group: ChartGroup) -> go.Figure:
    series_names, sizes, by_series = decompose_scaling(group)
    if not sizes or not series_names:
        return go.Figure()

    # Detect if we should facet: if series names contain "/" they have a sub-category
    facet_keys: dict[str, list[str]] = defaultdict(list)
    for s in series_names:
        parts = s.split(" / ")
        cat = parts[0] if len(parts) > 1 else ""
        facet_keys[cat].append(s)

    use_facets = len(facet_keys) > 1
    if use_facets:
        facet_labels = sorted(facet_keys)
        cols = min(len(facet_labels), 3)
        rows = (len(facet_labels) + cols - 1) // cols
        fig = make_subplots(
            rows=rows, cols=cols,
            subplot_titles=[_pretty_title(f) for f in facet_labels],
            horizontal_spacing=0.10, vertical_spacing=0.18,
        )
        for facet_idx, facet in enumerate(facet_labels):
            r, c = facet_idx // cols + 1, facet_idx % cols + 1
            for s_idx, series in enumerate(facet_keys[facet]):
                pts = by_series[series]
                short_name = series.split(" / ")[-1] if " / " in series else series
                fig.add_trace(go.Scatter(
                    x=[p.size for p in pts], y=[p.value_s for p in pts],
                    mode="lines+markers", name=short_name,
                    line=dict(color=_color_for(s_idx), width=2.5),
                    marker=dict(size=7),
                    hovertext=[f"<b>{short_name}</b> @ {p.size:,}<br>{format_duration(p.value_s)}" for p in pts],
                    hoverinfo="text",
                    legendgroup=short_name, showlegend=(facet_idx == 0),
                ), row=r, col=c)
    else:
        rows, cols = 1, 1
        fig = go.Figure()
        for s_idx, series in enumerate(series_names):
            pts = by_series[series]
            fig.add_trace(go.Scatter(
                x=[p.size for p in pts], y=[p.value_s for p in pts],
                mode="lines+markers", name=series,
                line=dict(color=_color_for(s_idx), width=2.5),
                marker=dict(size=7),
                hovertext=[f"<b>{series}</b> @ {p.size:,}<br>{format_duration(p.value_s)}" for p in pts],
                hoverinfo="text",
            ))

    fig.update_xaxes(type="log", dtick=1, gridcolor="#e8e8e8", title_standoff=10)
    fig.update_yaxes(type="log", exponentformat="SI", gridcolor="#e8e8e8", title_standoff=10)

    for ann in fig.layout.annotations:
        ann.font.size = 15
        ann.font.color = "#333"

    n_legend = len(series_names)
    legend_rows = (n_legend + 3) // 4

    fig.update_layout(
        **{**LAYOUT_DEFAULTS, "margin": dict(t=80 + legend_rows * 22, b=60, l=70, r=30)},
        title=dict(text=group.title, x=0.5, font=dict(size=18, color="#222")),
        height=max(450, 420 * rows) + legend_rows * 22,
        legend=dict(
            orientation="h", yanchor="bottom", y=1.0, xanchor="center", x=0.5,
            font=dict(size=12),
        ),
    )
    return fig


def build_bar_fig(group: ChartGroup) -> go.Figure:
    grp_names, labels, grid = decompose_bars(group)
    if not labels:
        return go.Figure()

    fig = go.Figure()
    if len(grp_names) <= 1:
        grp = grp_names[0] if grp_names else ""
        vals = [grid[grp].get(lbl) for lbl in labels]
        fig.add_trace(go.Bar(
            x=[_pretty_title(lbl) for lbl in labels],
            y=[v.value_s if v else None for v in vals],
            text=[format_duration(v.value_s) if v else "" for v in vals],
            textposition="outside", textfont_size=11,
            marker_color=SERIES_PALETTE[0],
            hovertemplate="<b>%{x}</b><br>%{text}<extra></extra>",
        ))
    else:
        for g_idx, grp in enumerate(grp_names):
            vals = [grid[grp].get(lbl) for lbl in labels]
            fig.add_trace(go.Bar(
                x=[_pretty_title(lbl) for lbl in labels],
                y=[v.value_s if v else None for v in vals],
                name=_pretty_title(grp),
                text=[format_duration(v.value_s) if v else "" for v in vals],
                textposition="outside", textfont_size=11,
                marker_color=SERIES_PALETTE[g_idx % len(SERIES_PALETTE)],
                hovertemplate="<b>%{x}</b><br>%{text}<extra>" + _pretty_title(grp) + "</extra>",
            ))

    fig.update_layout(
        **LAYOUT_DEFAULTS,
        title=dict(text=group.title, x=0.5, font=dict(size=18, color="#222")),
        barmode="group", bargap=0.20, bargroupgap=0.06,
        yaxis=dict(type="log", title="time (s)", exponentformat="SI", gridcolor="#e8e8e8"),
        xaxis=dict(tickfont=dict(size=13)),
        height=480,
        legend=dict(
            orientation="h", yanchor="bottom", y=1.02, xanchor="center", x=0.5,
            font=dict(size=13),
        ),
    )
    return fig


def build_delta_fig(baseline: AggregateFile, candidate: AggregateFile) -> go.Figure:
    shared = sorted(set(baseline.rows) & set(candidate.rows))
    deltas = []
    for key in shared:
        bv = baseline.rows[key].value_s
        cv = candidate.rows[key].value_s
        if bv == 0:
            continue
        pct = ((cv - bv) / bv) * 100.0
        deltas.append((key, pct, bv, cv))
    deltas.sort(key=lambda x: x[1])

    labels = [d[0] for d in deltas]
    pcts = [d[1] for d in deltas]
    colors = ["#54A24B" if p <= 0 else "#E45756" for p in pcts]
    hovers = [
        f"<b>{k}</b><br>{format_duration(bv)} -> {format_duration(cv)}<br>{pct:+.1f}%"
        for k, pct, bv, cv in deltas
    ]

    fig = go.Figure(go.Bar(
        x=pcts, y=labels, orientation="h",
        marker_color=colors,
        hovertext=hovers, hoverinfo="text",
    ))
    fig.update_layout(
        **{**LAYOUT_DEFAULTS, "margin": dict(l=340, t=80, b=50, r=30)},
        title=dict(text="Performance Delta", x=0.5, font=dict(size=18, color="#222")),
        xaxis=dict(
            title="change %", zeroline=True, zerolinecolor="#333", zerolinewidth=2,
            gridcolor="#e8e8e8",
        ),
        height=max(400, 24 * len(deltas) + 120),
        yaxis=dict(autorange="reversed", tickfont=dict(size=11)),
    )
    return fig


def build_comparison_scaling_fig(base_group: ChartGroup, cand_group: ChartGroup) -> go.Figure:
    b_series, b_sizes, b_data = decompose_scaling(base_group)
    c_series, c_sizes, c_data = decompose_scaling(cand_group)
    all_series = list(dict.fromkeys(b_series + c_series))

    def _short(name: str) -> str:
        parts = name.split(" / ")
        return parts[-1] if len(parts) > 1 else name

    fig = go.Figure()
    for s_idx, series in enumerate(all_series):
        color = _color_for(s_idx)
        short = _short(series)
        if series in b_data:
            pts = b_data[series]
            fig.add_trace(go.Scatter(
                x=[p.size for p in pts], y=[p.value_s for p in pts],
                mode="lines", name=f"{short} base", legendgroup=series,
                line=dict(color=color, width=1.5, dash="dash"), opacity=0.45,
            ))
        if series in c_data:
            pts = c_data[series]
            fig.add_trace(go.Scatter(
                x=[p.size for p in pts], y=[p.value_s for p in pts],
                mode="lines+markers", name=f"{short} cand", legendgroup=series,
                line=dict(color=color, width=2.5), marker=dict(size=6),
            ))

    n_legend_items = sum((s in b_data) + (s in c_data) for s in all_series)
    legend_rows = (n_legend_items + 3) // 4

    fig.update_xaxes(type="log", dtick=1, gridcolor="#e8e8e8", title_standoff=10)
    fig.update_yaxes(type="log", exponentformat="SI", gridcolor="#e8e8e8", title_standoff=10)
    fig.update_layout(
        **{**LAYOUT_DEFAULTS, "margin": dict(t=80 + legend_rows * 24, b=60, l=70, r=30)},
        title=dict(text=f"{base_group.title} — Comparison", x=0.5, font=dict(size=18, color="#222")),
        height=500 + legend_rows * 24,
        legend=dict(
            orientation="h", yanchor="bottom", y=1.0, xanchor="center", x=0.5,
            font=dict(size=12), tracegroupgap=4,
        ),
    )
    return fig


# =============================================================================
# HTML assembly
# =============================================================================

def _metadata_html(agg: AggregateFile, label: str = "") -> str:
    prefix = f"<h3 style='color:#333;margin-bottom:8px'>{label}</h3>" if label else ""
    rows = "".join(
        f"<tr><td style='padding:3px 14px;color:#666;font-weight:500'>{k}</td>"
        f"<td style='padding:3px 14px;color:#222'>{v}</td></tr>"
        for k, v in agg.metadata.items()
    )
    return f"{prefix}<table style='font-family:inherit;font-size:13px;border-collapse:collapse'>{rows}</table>"


def _figures_to_html(figures: list[go.Figure]) -> str:
    sections = []
    for i, fig in enumerate(figures):
        plotlyjs = "cdn" if i == 0 else False
        sections.append(f"<div class='section'>{fig.to_html(full_html=False, include_plotlyjs=plotlyjs)}</div>")
    return "\n".join(sections)


def _wrap_page(title: str, subtitle: str, meta_html: str, body: str) -> str:
    return f"""<!DOCTYPE html>
<html><head>
<meta charset="utf-8">
<title>{title}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;700&display=swap" rel="stylesheet">
<style>{PAGE_STYLE}</style>
</head><body>
<h1>{title}</h1>
<p class="subtitle">{subtitle}</p>
<div class="section">{meta_html}</div>
{body}
</body></html>"""


def assemble_single(agg: AggregateFile) -> str:
    groups = classify_groups(agg)
    figures: list[go.Figure] = []
    for group in groups:
        if group.kind == GroupKind.SCALING:
            figures.append(build_scaling_fig(group))
        else:
            figures.append(build_bar_fig(group))

    subtitle = f"{agg.metadata.get('Preset', '')} · {agg.metadata.get('Date', '')} · {agg.metadata.get('Hardware', '')}"
    return _wrap_page("Matcher Benchmark Dashboard", subtitle, _metadata_html(agg, "Run Metadata"), _figures_to_html(figures))


def assemble_comparison(baseline: AggregateFile, candidate: AggregateFile) -> str:
    figures: list[go.Figure] = [build_delta_fig(baseline, candidate)]

    b_groups = {g.title: g for g in classify_groups(baseline)}
    c_groups = {g.title: g for g in classify_groups(candidate)}
    for title in sorted(b_groups):
        if title in c_groups and b_groups[title].kind == GroupKind.SCALING:
            figures.append(build_comparison_scaling_fig(b_groups[title], c_groups[title]))

    for g in classify_groups(candidate):
        if g.kind == GroupKind.BAR:
            figures.append(build_bar_fig(g))

    meta_html = f"<div class='meta-row'>{_metadata_html(baseline, 'Baseline')}{_metadata_html(candidate, 'Candidate')}</div>"
    return _wrap_page("Benchmark Comparison", "baseline vs candidate", meta_html, _figures_to_html(figures))


# =============================================================================
# CLI
# =============================================================================

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Visualize benchmark results as interactive HTML charts.")
    parser.add_argument("inputs", nargs="+", type=pathlib.Path, help="One path for single-run view, two for comparison.")
    parser.add_argument("--output", "-o", type=pathlib.Path, default=None, help="Output HTML path.")
    parser.add_argument("--metric", choices=METRIC_CHOICES, default="median")
    parser.add_argument("--open", action="store_true", help="Open result in browser.")
    return parser


def main() -> int:
    args = build_parser().parse_args()

    if len(args.inputs) == 1:
        agg = load_aggregate_input(args.inputs[0], metric=args.metric)
        html = assemble_single(agg)
        default_name = "bench_viz.html"
        input_dir = args.inputs[0] if args.inputs[0].is_dir() else args.inputs[0].parent
    elif len(args.inputs) == 2:
        baseline = load_aggregate_input(args.inputs[0], metric=args.metric)
        candidate = load_aggregate_input(args.inputs[1], metric=args.metric)
        html = assemble_comparison(baseline, candidate)
        default_name = "bench_compare_viz.html"
        input_dir = args.inputs[1] if args.inputs[1].is_dir() else args.inputs[1].parent
    else:
        print("Error: provide 1 path (single view) or 2 paths (comparison).")
        return 1

    output = args.output or (input_dir / default_name)
    output.write_text(html, encoding="utf-8")
    print(f"Written: {output}")

    if args.open:
        webbrowser.open(output.resolve().as_uri())

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
