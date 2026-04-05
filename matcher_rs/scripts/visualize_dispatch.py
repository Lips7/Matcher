#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = ["plotly"]
# ///
"""Engine dispatch characterization visualizer.

Reads CSV from characterize_engines and generates an interactive HTML dashboard
with heatmaps showing engine throughput across the dispatch matrix.

Usage:
    python3 visualize_dispatch.py dispatch.csv
    python3 visualize_dispatch.py dispatch.csv --open
    python3 visualize_dispatch.py dispatch.csv -o custom_output.html
"""
from __future__ import annotations

import argparse
import csv
import pathlib
import webbrowser
from collections import defaultdict

import plotly.graph_objects as go
from plotly.subplots import make_subplots

ENGINE_COLORS = {
    "ac_dfa": "#1f77b4",
    "daac_bytewise": "#ff7f0e",
    "daac_charwise": "#2ca02c",
    "harry": "#d62728",
}

WINNER_COLORS = {
    "ac_dfa": "blue",
    "daac_bytewise": "orange",
    "daac_charwise": "green",
    "harry": "red",
}


def load_csv(path: pathlib.Path) -> list[dict]:
    rows = []
    with open(path) as f:
        reader = csv.DictReader(f)
        for row in reader:
            rows.append({
                "engine": row["engine"],
                "mode": row["mode"],
                "n": int(row["n"]),
                "pat_cjk": int(row["pat_cjk"]),
                "text_cjk": int(row["text_cjk"]),
                "median_us": float(row["median_us"]),
                "throughput_mbps": float(row["throughput_mbps"]),
            })
    return rows


def group_by(rows: list[dict], *keys: str) -> dict:
    groups: dict = defaultdict(list)
    for row in rows:
        key = tuple(row[k] for k in keys)
        groups[key].append(row)
    return groups


def unique_sorted(rows: list[dict], key: str) -> list:
    return sorted(set(row[key] for row in rows))


def make_throughput_heatmaps(rows: list[dict], mode: str, pat_cjk: int) -> go.Figure:
    """Per-engine throughput heatmap: text_cjk% (x) × pattern_size (y)."""
    mode_rows = [r for r in rows if r["mode"] == mode and r["pat_cjk"] == pat_cjk]
    engines = unique_sorted(mode_rows, "engine")
    n_engines = len(engines)
    if n_engines == 0:
        return go.Figure()

    fig = make_subplots(
        rows=1, cols=n_engines,
        subplot_titles=[e for e in engines],
        horizontal_spacing=0.05,
    )

    sizes = unique_sorted(mode_rows, "n")
    text_cjks = unique_sorted(mode_rows, "text_cjk")

    for idx, engine in enumerate(engines):
        engine_rows = [r for r in mode_rows if r["engine"] == engine]
        lookup = {(r["n"], r["text_cjk"]): r["throughput_mbps"] for r in engine_rows}

        z = []
        for n in sizes:
            row_vals = [lookup.get((n, tc), None) for tc in text_cjks]
            z.append(row_vals)

        fig.add_trace(
            go.Heatmap(
                z=z,
                x=[str(tc) for tc in text_cjks],
                y=[str(n) for n in sizes],
                colorscale="Viridis",
                colorbar=dict(title="MB/s", x=1.0 if idx == n_engines - 1 else None),
                showscale=(idx == n_engines - 1),
                hovertemplate="text_cjk=%{x}%<br>n=%{y}<br>%{z:.0f} MB/s<extra></extra>",
            ),
            row=1, col=idx + 1,
        )

    fig.update_layout(
        title=f"Engine Throughput — {mode}, pat_cjk={pat_cjk}%",
        height=600,
        width=350 * n_engines,
    )
    for i in range(n_engines):
        fig.update_xaxes(title_text="text CJK %", row=1, col=i + 1)
        fig.update_yaxes(title_text="pattern count", row=1, col=i + 1)

    return fig


def make_winner_heatmap(rows: list[dict], mode: str, pat_cjk: int) -> go.Figure:
    """Which engine is fastest at each (n, text_cjk) point."""
    mode_rows = [r for r in rows if r["mode"] == mode and r["pat_cjk"] == pat_cjk]
    sizes = unique_sorted(mode_rows, "n")
    text_cjks = unique_sorted(mode_rows, "text_cjk")
    engines = unique_sorted(mode_rows, "engine")

    engine_to_idx = {e: i for i, e in enumerate(engines)}

    lookup: dict[tuple, dict] = defaultdict(dict)
    for r in mode_rows:
        lookup[(r["n"], r["text_cjk"])][r["engine"]] = r["throughput_mbps"]

    z_winner = []
    z_margin = []
    hover_text = []
    for n in sizes:
        winner_row = []
        margin_row = []
        hover_row = []
        for tc in text_cjks:
            throughputs = lookup.get((n, tc), {})
            if not throughputs:
                winner_row.append(-1)
                margin_row.append(0)
                hover_row.append("")
                continue
            best_engine = max(throughputs, key=throughputs.get)
            best_val = throughputs[best_engine]
            second_val = max(v for e, v in throughputs.items() if e != best_engine) if len(throughputs) > 1 else best_val
            margin = ((best_val - second_val) / second_val * 100) if second_val > 0 else 0

            winner_row.append(engine_to_idx[best_engine])
            margin_row.append(margin)

            detail = "<br>".join(f"  {e}: {v:.0f} MB/s" for e, v in sorted(throughputs.items(), key=lambda x: -x[1]))
            hover_row.append(f"<b>{best_engine}</b> (+{margin:.0f}%)<br>{detail}")

        z_winner.append(winner_row)
        z_margin.append(margin_row)
        hover_text.append(hover_row)

    colorscale = [[i / max(len(engines) - 1, 1), ENGINE_COLORS.get(e, "#999")] for i, e in enumerate(engines)]

    fig = go.Figure(data=go.Heatmap(
        z=z_winner,
        x=[str(tc) for tc in text_cjks],
        y=[str(n) for n in sizes],
        colorscale=colorscale,
        zmin=0, zmax=len(engines) - 1,
        colorbar=dict(
            title="Engine",
            tickvals=list(range(len(engines))),
            ticktext=engines,
        ),
        text=hover_text,
        hovertemplate="%{text}<extra></extra>",
    ))

    fig.update_layout(
        title=f"Fastest Engine — {mode}, pat_cjk={pat_cjk}%",
        xaxis_title="text CJK %",
        yaxis_title="pattern count",
        height=600,
        width=700,
    )
    return fig


def make_crossover_charts(rows: list[dict], mode: str, pat_cjk: int) -> go.Figure:
    """Throughput vs text_cjk% for each pattern size, all engines overlaid."""
    mode_rows = [r for r in rows if r["mode"] == mode and r["pat_cjk"] == pat_cjk]
    sizes = unique_sorted(mode_rows, "n")
    engines = unique_sorted(mode_rows, "engine")
    text_cjks = unique_sorted(mode_rows, "text_cjk")

    n_sizes = len(sizes)
    cols = min(4, n_sizes)
    chart_rows = (n_sizes + cols - 1) // cols

    fig = make_subplots(
        rows=chart_rows, cols=cols,
        subplot_titles=[f"n={n}" for n in sizes],
        horizontal_spacing=0.06,
        vertical_spacing=0.08,
    )

    for si, n in enumerate(sizes):
        r_idx = si // cols + 1
        c_idx = si % cols + 1
        for engine in engines:
            e_rows = [r for r in mode_rows if r["engine"] == engine and r["n"] == n]
            e_lookup = {r["text_cjk"]: r["throughput_mbps"] for r in e_rows}
            x = [tc for tc in text_cjks if tc in e_lookup]
            y = [e_lookup[tc] for tc in x]
            fig.add_trace(
                go.Scatter(
                    x=x, y=y,
                    mode="lines+markers",
                    name=engine,
                    legendgroup=engine,
                    showlegend=(si == 0),
                    line=dict(color=ENGINE_COLORS.get(engine, "#999")),
                    hovertemplate=f"{engine}<br>text_cjk=%{{x}}%<br>%{{y:.0f}} MB/s<extra></extra>",
                ),
                row=r_idx, col=c_idx,
            )

    fig.update_layout(
        title=f"Engine Crossover — {mode}, pat_cjk={pat_cjk}%",
        height=300 * chart_rows,
        width=300 * cols,
    )
    fig.update_xaxes(title_text="text CJK %")
    fig.update_yaxes(title_text="MB/s")
    return fig


def make_pat_composition_chart(rows: list[dict], mode: str, engine: str, n: int) -> go.Figure:
    """Throughput vs pat_cjk% for a fixed engine and pattern size."""
    e_rows = [r for r in rows if r["mode"] == mode and r["engine"] == engine and r["n"] == n]
    text_cjks = unique_sorted(e_rows, "text_cjk")
    pat_cjks = unique_sorted(e_rows, "pat_cjk")

    fig = go.Figure()
    colors = [f"hsl({int(tc / 100 * 240)}, 70%, 50%)" for tc in text_cjks]

    for i, tc in enumerate(text_cjks):
        tc_rows = [r for r in e_rows if r["text_cjk"] == tc]
        lookup = {r["pat_cjk"]: r["throughput_mbps"] for r in tc_rows}
        x = [pc for pc in pat_cjks if pc in lookup]
        y = [lookup[pc] for pc in x]
        fig.add_trace(go.Scatter(
            x=x, y=y,
            mode="lines+markers",
            name=f"text_cjk={tc}%",
            line=dict(color=colors[i]),
        ))

    fig.update_layout(
        title=f"Pattern Composition Effect — {engine}, {mode}, n={n}",
        xaxis_title="pattern CJK %",
        yaxis_title="MB/s",
        height=500,
        width=800,
    )
    return fig


def build_dashboard(rows: list[dict], output: pathlib.Path):
    modes = unique_sorted(rows, "mode")
    pat_cjks = unique_sorted(rows, "pat_cjk")
    engines = unique_sorted(rows, "engine")
    sizes = unique_sorted(rows, "n")

    html_parts = [
        "<html><head>",
        '<script src="https://cdn.plot.ly/plotly-latest.min.js"></script>',
        "<style>body{font-family:sans-serif;margin:20px;} .chart{margin-bottom:40px;}</style>",
        "</head><body>",
        "<h1>Engine Dispatch Characterization</h1>",
        f"<p>{len(rows)} data points: {len(engines)} engines, {len(sizes)} sizes, "
        f"{len(pat_cjks)} pat_cjk levels, {len(unique_sorted(rows, 'text_cjk'))} text_cjk levels</p>",
    ]

    chart_id = 0

    for mode in modes:
        html_parts.append(f"<h2>{mode}</h2>")

        # Winner heatmap (pat_cjk=0 as primary)
        fig = make_winner_heatmap(rows, mode, pat_cjk=0)
        html_parts.append(f'<div class="chart" id="chart{chart_id}"></div>')
        html_parts.append(f"<script>Plotly.newPlot('chart{chart_id}', {fig.to_json()});</script>")
        chart_id += 1

        # Throughput heatmaps (pat_cjk=0)
        fig = make_throughput_heatmaps(rows, mode, pat_cjk=0)
        html_parts.append(f'<div class="chart" id="chart{chart_id}"></div>')
        html_parts.append(f"<script>Plotly.newPlot('chart{chart_id}', {fig.to_json()});</script>")
        chart_id += 1

        # Crossover charts (pat_cjk=0)
        fig = make_crossover_charts(rows, mode, pat_cjk=0)
        html_parts.append(f'<div class="chart" id="chart{chart_id}"></div>')
        html_parts.append(f"<script>Plotly.newPlot('chart{chart_id}', {fig.to_json()});</script>")
        chart_id += 1

        # Pattern composition for DFA at a representative size
        representative_n = sizes[len(sizes) // 2] if sizes else 2000
        for engine in engines:
            fig = make_pat_composition_chart(rows, mode, engine, representative_n)
            html_parts.append(f'<div class="chart" id="chart{chart_id}"></div>')
            html_parts.append(f"<script>Plotly.newPlot('chart{chart_id}', {fig.to_json()});</script>")
            chart_id += 1

    html_parts.append("</body></html>")

    output.write_text("\n".join(html_parts))
    print(f"Wrote {output} ({chart_id} charts)")


def main():
    parser = argparse.ArgumentParser(description="Visualize engine dispatch characterization")
    parser.add_argument("csv_file", type=pathlib.Path, help="CSV from characterize_engines")
    parser.add_argument("-o", "--output", type=pathlib.Path, default=None, help="Output HTML path")
    parser.add_argument("--open", action="store_true", help="Open in browser")
    args = parser.parse_args()

    rows = load_csv(args.csv_file)
    print(f"Loaded {len(rows)} data points from {args.csv_file}")

    output = args.output or args.csv_file.with_suffix(".html")
    build_dashboard(rows, output)

    if args.open:
        webbrowser.open(str(output.resolve()))


if __name__ == "__main__":
    main()
