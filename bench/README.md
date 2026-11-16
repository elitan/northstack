# Benchmark: northstack vs Next.js + Postgres

The same small app (the notes example: same API, same queries) built twice:

- **northstack**: one Rust binary with SQLite, the image from this repo's
  `Dockerfile`.
- **Next.js 16** (production build, `output: standalone`) with Kysely and
  **Postgres 16**: [`nextjs/`](nextjs/), schema in [`pg-schema.sql`](pg-schema.sql).
  Both apps render the page in the browser: the server sends a static page and
  the browser loads the data from the API.

Each container is limited to **1 CPU and 512 MB** (Docker `--cpus 1 --memory
512m`). The Next.js stack is two containers (Next.js and Postgres), so it may
use up to 2 CPUs; its numbers below count both. Both start with the same 1,000
notes. The load generator, [oha](https://github.com/hatoo/oha), runs in its
own container. Each test runs 3 s of warm-up, then 12 s measured. Read tests
run first at every concurrency level; create tests run last, so the rows they
add (and northstack's background jobs) don't affect the read tests.
Northstack's create also queues a background job per note (word count), which
the Next.js app doesn't do.

Measured on 2026-11-15 on an Apple M5 laptop (Docker Desktop, 10 CPUs) with
other apps running. Each number is the median of 3 full runs; raw data in
[`results/2026-11-15/`](results/2026-11-15/). The first run's 1-user numbers
were disturbed by other load on the machine; runs 2 and 3 agree within a few
percent.

## Results

| | northstack | Next.js + Postgres |
|---|---:|---:|
| Memory at rest, fresh start | 2.9 MB | 66 MB |
| Memory at rest, 1,000 rows | 6.3 MB | 134 MB |
| Peak memory, reads at 50 users | 12 MB | 195 MB |
| Start until it answers `/readyz` | 0.11 s | 0.28 s |
| Image size | 55 MB | 385 MB + 627 MB (Postgres) |

**1 concurrent user**

| Test | northstack req/s | Next.js req/s | | p99 northstack | p99 Next.js | CPU northstack | CPU Next.js + Postgres |
|---|---:|---:|---:|---:|---:|---:|---:|
| Serve the page | 12,152 | 3,310 | 3.7x | 0.1 ms | 0.7 ms | 68% | 98% |
| Get one row | 7,297 | 3,467 | 2.1x | 0.2 ms | 0.6 ms | 76% | 105% |
| List 50 rows | 3,470 | 1,989 | 1.7x | 0.3 ms | 1.6 ms | 98% | 110% |
| List 200 rows | 1,028 | 868 | 1.2x | 1.4 ms | 14.8 ms | 97% | 109% |
| Stats (count + sum) | 6,121 | 3,684 | 1.7x | 0.2 ms | 0.6 ms | 79% | 116% |
| Create a row (+ background job) | 1,486 | 1,975 | 0.8x | 8.8 ms | 1.3 ms | 83% | 95% |

**10 concurrent users**

| Test | northstack req/s | Next.js req/s | | p99 northstack | p99 Next.js | CPU northstack | CPU Next.js + Postgres |
|---|---:|---:|---:|---:|---:|---:|---:|
| Serve the page | 57,744 | 3,840 | 15.0x | 0.2 ms | 47.4 ms | 97% | 96% |
| Get one row | 15,244 | 3,769 | 4.0x | 0.9 ms | 34.7 ms | 95% | 109% |
| List 50 rows | 4,860 | 1,893 | 2.6x | 32.5 ms | 43.7 ms | 98% | 109% |
| List 200 rows | 1,117 | 768 | 1.5x | 45.4 ms | 59.9 ms | 98% | 110% |
| Stats (count + sum) | 8,612 | 3,926 | 2.2x | 35.2 ms | 37.5 ms | 96% | 132% |
| Create a row (+ background job) | 2,664 | 2,133 | 1.2x | 54.9 ms | 42.5 ms | 85% | 128% |

**50 concurrent users**

| Test | northstack req/s | Next.js req/s | | p99 northstack | p99 Next.js | CPU northstack | CPU Next.js + Postgres |
|---|---:|---:|---:|---:|---:|---:|---:|
| Serve the page | 39,836 | 3,210 | 12.4x | 1.4 ms | 74.6 ms | 95% | 96% |
| Get one row | 12,028 | 2,820 | 4.3x | 25.5 ms | 67.2 ms | 97% | 109% |
| List 50 rows | 4,868 | 1,649 | 3.0x | 44.8 ms | 75.2 ms | 95% | 109% |
| List 200 rows | 1,133 | 746 | 1.5x | 73.0 ms | 100.2 ms | 97% | 108% |
| Stats (count + sum) | 8,235 | 3,120 | 2.6x | 52.0 ms | 62.6 ms | 98% | 130% |
| Create a row (+ background job) | 3,391 | 1,884 | 1.8x | 53.4 ms | 68.4 ms | 85% | 132% |

Failed requests across all runs: 0.


`p99` is the slowest 1% of requests. CPU is the average during the test (100%
= one core); the Next.js stack can use up to 2 (Next.js and Postgres).

**Where Next.js wins:** creating a row with one user at a time (1,975 vs 1,486
requests/s, and a slowest 1% of 1.3 ms vs 8.8 ms). Each northstack create also
queues a background job that a worker runs right away, and SQLite allows one
writer at a time, so requests and jobs take turns writing. The Next.js app
writes the job row but never runs it. From 10 users up, northstack is faster
at creating too.

## Run it yourself

Needs Docker and Python 3.

```sh
docker build -t nsbench-ns .                       # from the repo root
docker build -t nsbench-next bench/nextjs
python3 bench/bench.py --out bench/results/mine-1.json   # about 20 minutes per run
python3 bench/summarize.py bench/results/mine-1.json     # one run, side by side
python3 bench/report.py bench/results/mine-*.json         # median of several runs
```

It creates containers named `nsbench-*` on a `nsbench` network and publishes
ports 47391 and 47392; remove them afterwards with
`docker rm -f nsbench-ns nsbench-next nsbench-pg`.
