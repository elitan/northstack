#!/usr/bin/env python3
"""Local benchmark: northstack (Rust + SQLite) vs Next.js 16 + Kysely + Postgres 16.

Every container gets the same limits (--cpus/--memory). The load generator (oha)
runs in its own container on the same Docker network, without limits. Docker
stats are sampled about once a second; each load window reports requests/s,
latency percentiles, errors, average CPU and peak memory of the whole stack
(Next.js counts Next.js + Postgres; northstack is one container).

Usage: python3 bench.py [--duration 15] [--levels 1,10,50]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import threading
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
NET = "nsbench"
CPUS, MEM = "1", "512m"
OHA = "ghcr.io/hatoo/oha:latest"
STACKS = {
    "northstack": {"containers": ["nsbench-ns"], "url": "http://nsbench-ns:3000", "port": 47391},
    "nextjs": {"containers": ["nsbench-next", "nsbench-pg"], "url": "http://nsbench-next:3000", "port": 47392},
}


def sh(cmd: list[str], check=True, capture=True) -> str:
    p = subprocess.run(cmd, capture_output=capture, text=True)
    if check and p.returncode:
        raise SystemExit(f"failed: {' '.join(cmd)}\n{p.stdout}{p.stderr}")
    return (p.stdout or "").strip()


def wait_ready(port: int, path="/readyz", timeout=60.0) -> float:
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}{path}", timeout=1) as r:
                if r.status == 200:
                    return time.time() - t0
        except Exception:
            pass
        time.sleep(0.02)
    raise SystemExit(f"port {port} not ready after {timeout}s")


def rpc(port: int, name: str, body) -> dict:
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/rpc/{name}", data=json.dumps(body).encode(),
        headers={"content-type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


def mem_mib(s: str) -> float:
    v = s.split("/")[0].strip()
    for unit, f in (("GiB", 1024), ("MiB", 1), ("KiB", 1 / 1024), ("kB", 1 / 1000), ("MB", 1), ("GB", 1000), ("B", 1 / 1024 / 1024)):
        if v.endswith(unit):
            return float(v[: -len(unit)]) * f
    return float(v)


class Sampler(threading.Thread):
    """Samples docker stats for all bench containers, ~1/s."""

    def __init__(self):
        super().__init__(daemon=True)
        self.samples: list[tuple[float, str, float, float]] = []
        self.stop = False

    def run(self):
        names = [c for s in STACKS.values() for c in s["containers"]]
        while not self.stop:
            out = subprocess.run(
                ["docker", "stats", "--no-stream", "--format", "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}", *names],
                capture_output=True, text=True).stdout
            t = time.time()
            for line in out.splitlines():
                try:
                    n, cpu, mem = line.split("|")
                    self.samples.append((t, n, float(cpu.rstrip("%")), mem_mib(mem)))
                except ValueError:
                    pass

    def window(self, stack: str, t0: float, t1: float) -> dict:
        cs = STACKS[stack]["containers"]
        per_t: dict[float, list] = {}
        for t, n, cpu, mem in self.samples:
            if t0 <= t <= t1 and n in cs:
                per_t.setdefault(t, []).append((n, cpu, mem))
        rows = [v for v in per_t.values() if len(v) == len(cs)]
        if not rows:
            return {"cpu_avg": None, "mem_peak": None, "mem_avg": None}
        cpu = [sum(x[1] for x in r) for r in rows]
        mem = [sum(x[2] for x in r) for r in rows]
        per_container = {
            n: round(max(x[2] for r in rows for x in r if x[0] == n), 1) for n in cs
        }
        return {"cpu_avg": round(sum(cpu) / len(cpu), 1), "mem_peak": round(max(mem), 1),
                "mem_avg": round(sum(mem) / len(mem), 1), "mem_peak_by_container": per_container}


def oha(url: str, endpoint: str, c: int, secs: int, note_id: str) -> dict:
    bodies = {
        "list50": ("notes.list", {"limit": 50}),
        "list200": ("notes.list", {"limit": 200}),
        "stats": ("notes.stats", None),
        "get": ("notes.get", {"id": note_id}),
        "create": ("notes.create", {"title": "load test", "body": "one two three four five six seven"}),
    }
    base = ["docker", "run", "--rm", "--network", NET, OHA, "--no-tui", "-c", str(c)]
    if endpoint == "page":
        args = [f"{url}/"]
    else:
        name, body = bodies[endpoint]
        args = ["-m", "POST", "-H", "content-type: application/json", "-d", json.dumps(body), f"{url}/rpc/{name}"]
    # Warm-up, then the measured run.
    subprocess.run(base + ["-z", "3s", "--output-format", "quiet"] + args, capture_output=True)
    for attempt in range(2):
        p = subprocess.run(base + ["-z", f"{secs}s", "--output-format", "json"] + args, capture_output=True, text=True)
        try:
            j = json.loads(p.stdout)
            break
        except json.JSONDecodeError:
            print(f"oha gave no result ({p.stderr.strip()[:200]}); retrying", flush=True)
            time.sleep(5)
    else:
        return {"rps": None, "p50_ms": None, "p99_ms": None, "ok": 0, "errors": None, "failed": p.stderr.strip()[:300]}
    codes = j.get("statusCodeDistribution", {})
    ok = sum(v for k, v in codes.items() if k.startswith("2"))
    # The request cut off when the -z deadline hits isn't an error.
    errs = {k: v for k, v in j.get("errorDistribution", {}).items() if "deadline" not in k}
    err = sum(v for k, v in codes.items() if not k.startswith("2")) + sum(errs.values())
    lp = j.get("latencyPercentiles", {})
    return {"rps": round(j["summary"]["requestsPerSec"], 1), "p50_ms": round((lp.get("p50") or 0) * 1000, 1),
            "p99_ms": round((lp.get("p99") or 0) * 1000, 1), "ok": ok, "errors": err}


def setup(ns_image: str, next_image: str):
    sh(["docker", "rm", "-f", "nsbench-ns", "nsbench-next", "nsbench-pg"], check=False)
    sh(["docker", "volume", "rm", "nsbench-ns-data"], check=False)
    sh(["docker", "network", "create", NET], check=False)
    sh(["docker", "pull", "-q", OHA])
    lim = ["--cpus", CPUS, "--memory", MEM, "--network", NET]
    sh(["docker", "run", "-d", "--name", "nsbench-pg", *lim, "-e", "POSTGRES_HOST_AUTH_METHOD=trust",
        "-v", f"{HERE / 'pg-schema.sql'}:/docker-entrypoint-initdb.d/schema.sql:ro", "postgres:16"])
    for _ in range(120):
        if subprocess.run(["docker", "exec", "nsbench-pg", "pg_isready", "-U", "postgres", "-h", "127.0.0.1"],
                          capture_output=True).returncode == 0 and \
           sh(["docker", "exec", "nsbench-pg", "psql", "-U", "postgres", "-tAc",
               "SELECT count(*) FROM pg_tables WHERE tablename='notes'"], check=False) == "1":
            break
        time.sleep(0.5)
    else:
        raise SystemExit("postgres did not start")
    starts = {}
    t0 = time.time()
    sh(["docker", "run", "-d", "--name", "nsbench-next", *lim, "-p", f"{STACKS['nextjs']['port']}:3000",
        "-e", "DATABASE_URL=postgres://postgres@nsbench-pg:5432/postgres", next_image])
    starts["nextjs"] = round(time.time() - t0 + wait_ready(STACKS["nextjs"]["port"]), 2)
    t0 = time.time()
    sh(["docker", "run", "-d", "--name", "nsbench-ns", *lim, "-p", f"{STACKS['northstack']['port']}:3000",
        "-v", "nsbench-ns-data:/data", ns_image])
    starts["northstack"] = round(time.time() - t0 + wait_ready(STACKS["northstack"]["port"]), 2)
    return starts


def seed(n: int) -> dict:
    ids = {}
    words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu".split()
    for stack in STACKS:
        port = STACKS[stack]["port"]
        for i in range(n):
            body = " ".join(words[(i + k) % len(words)] for k in range(5 + i % 20))
            prio = ["low", "normal", "high"][i % 3]
            note = rpc(port, "notes.create", {"title": f"Note {i}", "body": body, "priority": prio})
            ids.setdefault(stack, note["id"])
    # Next.js has no job worker: fill word counts the way northstack's job does.
    sh(["docker", "exec", "nsbench-pg", "psql", "-U", "postgres", "-qc",
        "UPDATE notes SET word_count = array_length(regexp_split_to_array(trim(body), '\\s+'), 1)"])
    sh(["docker", "exec", "nsbench-pg", "psql", "-U", "postgres", "-qc", "VACUUM ANALYZE"])
    # Wait for northstack's background jobs to finish.
    for _ in range(100):
        if rpc(STACKS["northstack"]["port"], "notes.stats", None)["words"] > 0 and \
           all(x["wordCount"] is not None for x in rpc(STACKS["northstack"]["port"], "notes.list", {"limit": 200})["notes"]):
            break
        time.sleep(0.2)
    return ids


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--duration", type=int, default=15)
    ap.add_argument("--levels", default="1,10,50")
    ap.add_argument("--endpoints", default="page,get,list50,list200,stats,create")
    ap.add_argument("--seed", type=int, default=1000)
    ap.add_argument("--ns-image", default="nsbench-ns")
    ap.add_argument("--next-image", default="nsbench-next")
    ap.add_argument("--out", default=str(HERE / "results.json"))
    a = ap.parse_args()

    result: dict = {"limits": {"cpus": CPUS, "memory": MEM}, "duration_s": a.duration}
    result["image_mib"] = {
        "northstack": round(int(sh(["docker", "image", "inspect", a.ns_image, "-f", "{{.Size}}"])) / 2**20, 1),
        "nextjs": round(int(sh(["docker", "image", "inspect", a.next_image, "-f", "{{.Size}}"])) / 2**20, 1),
        "postgres": round(int(sh(["docker", "image", "inspect", "postgres:16", "-f", "{{.Size}}"])) / 2**20, 1),
    }
    result["cold_start_s"] = setup(a.ns_image, a.next_image)
    print("cold start:", result["cold_start_s"], flush=True)

    sampler = Sampler()
    sampler.start()
    time.sleep(10)
    t = time.time()
    result["fresh_idle"] = {s: sampler.window(s, t - 8, t) for s in STACKS}
    ids = seed(a.seed)
    print("seeded", a.seed, "notes in each", flush=True)
    time.sleep(20)
    t = time.time()
    result["idle_after_seed"] = {s: sampler.window(s, t - 15, t) for s in STACKS}
    print("idle:", json.dumps(result["idle_after_seed"]), flush=True)

    runs = []
    # Reads first, at every level, on the same seeded data. Writes last: each
    # northstack create also queues a background job, and a job backlog (or a
    # grown table) would skew the read tests that follow.
    levels = [int(x) for x in a.levels.split(",")]
    eps = a.endpoints.split(",")
    reads = [e for e in eps if e != "create"]
    order = [(c, e) for c in levels for e in reads] + [(c, "create") for c in levels if "create" in eps]
    for c, ep in order:
            for stack in STACKS:
                t0 = time.time() + 3.5  # after the warm-up
                r = oha(STACKS[stack]["url"], ep, c, a.duration, ids[stack])
                t1 = time.time()
                r.update(stack=stack, endpoint=ep, concurrency=c, **sampler.window(stack, t0, t1))
                runs.append(r)
                print(json.dumps(r), flush=True)
                result["runs"] = runs
                Path(a.out).write_text(json.dumps(result, indent=2))
                time.sleep(2)
    result["runs"] = runs
    time.sleep(15)
    t = time.time()
    result["idle_after_load"] = {s: sampler.window(s, t - 12, t) for s in STACKS}
    sampler.stop = True
    Path(a.out).write_text(json.dumps(result, indent=2))
    print("wrote", a.out)


if __name__ == "__main__":
    main()
