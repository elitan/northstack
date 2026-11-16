#!/usr/bin/env python3
"""Combine benchmark runs (median of each number) into markdown tables.

Usage: python3 bench/report.py bench/results/2026-11-15/run*.json
"""

from __future__ import annotations

import json
import statistics
import sys

NAMES = {
    "page": "Serve the page",
    "get": "Get one row",
    "list50": "List 50 rows",
    "list200": "List 200 rows",
    "stats": "Stats (count + sum)",
    "create": "Create a row (+ background job)",
}


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def main(paths: list[str]) -> None:
    runs = [json.load(open(p)) for p in paths]
    print(f"Median of {len(runs)} runs.\n")

    def foot(key, stack, field="mem_avg"):
        return med([r[key][stack][field] for r in runs])

    reads = [e for e in NAMES if e != "create"]

    def cell(stack, ep, c, field):
        return med([x[field] for r in runs for x in r["runs"]
                    if x["stack"] == stack and x["endpoint"] == ep and x["concurrency"] == c])

    load_mem = {s: med([max(x["mem_peak"] for x in r["runs"] if x["stack"] == s and x["endpoint"] in reads
                            and x["concurrency"] == 50) for r in runs]) for s in ("northstack", "nextjs")}
    print("| | northstack | Next.js + Postgres |\n|---|---:|---:|")
    print(f"| Memory at rest, fresh start | {foot('fresh_idle', 'northstack'):.1f} MB | {foot('fresh_idle', 'nextjs'):.0f} MB |")
    print(f"| Memory at rest, 1,000 rows | {foot('idle_after_seed', 'northstack'):.1f} MB | {foot('idle_after_seed', 'nextjs'):.0f} MB |")
    print(f"| Peak memory, reads at 50 users | {load_mem['northstack']:.0f} MB | {load_mem['nextjs']:.0f} MB |")
    print(f"| Start until it answers `/readyz` | {med([r['cold_start_s']['northstack'] for r in runs]):.2f} s "
          f"| {med([r['cold_start_s']['nextjs'] for r in runs]):.2f} s |")
    im = runs[-1]["image_mib"]
    print(f"| Image size | {im['northstack']:.0f} MB | {im['nextjs']:.0f} MB + {im['postgres']:.0f} MB (Postgres) |")

    for c in (1, 10, 50):
        print(f"\n**{c} concurrent user{'s' if c > 1 else ''}**\n")
        print("| Test | northstack req/s | Next.js req/s | | p99 northstack | p99 Next.js | CPU northstack | CPU Next.js + Postgres |")
        print("|---|---:|---:|---:|---:|---:|---:|---:|")
        for ep, name in NAMES.items():
            n, m = cell("northstack", ep, c, "rps"), cell("nextjs", ep, c, "rps")
            print(f"| {name} | {n:,.0f} | {m:,.0f} | {n / m:.1f}x | {cell('northstack', ep, c, 'p99_ms'):.1f} ms "
                  f"| {cell('nextjs', ep, c, 'p99_ms'):.1f} ms | {cell('northstack', ep, c, 'cpu_avg'):.0f}% "
                  f"| {cell('nextjs', ep, c, 'cpu_avg'):.0f}% |")
    errors = sum(x["errors"] or 0 for r in runs for x in r["runs"])
    print(f"\nFailed requests across all runs: {errors}.")


if __name__ == "__main__":
    main(sys.argv[1:])
