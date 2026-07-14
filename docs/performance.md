# Performance

Measured with the built-in stress mode, which synthesizes a scenario with N
wandering tracks, 12 patrol assets, 4 protected zones, and 4 links, then runs
60 s of sim time (600 ticks) with all policies enabled and reports per-tick
wall time. Reproduce with:

```sh
cargo run --release -p dira-runtime -- benchmark --stress 10000
```

## Results

Apple M2, 16 GB, release build, 2026-07-14 (commit after the delivery-index fix):

| tracks | tick p50 | tick p95 | tick p99 | tick max | budget | snapshot size | serialize |
|-------:|---------:|---------:|---------:|---------:|-------:|--------------:|----------:|
|  1,000 |  0.67 ms |  0.92 ms |  1.52 ms | 12.8 ms  | 100 ms | 0.34 MB       | 0.5 ms    |
|  5,000 |  3.12 ms |  3.43 ms |  3.74 ms |  6.2 ms  | 100 ms | 1.69 MB       | 2.0 ms    |
| 10,000 |  6.25 ms |  6.97 ms |  7.12 ms | 10.4 ms  | 100 ms | 3.38 MB       | 4.4 ms    |

At 10,000 tracks the engine uses about 6% of its 100 ms tick budget, with
roughly linear scaling across the measured range.

## What the benchmark caught

The first run of this benchmark found a quadratic: observation delivery did a
linear scan of the track list per observation, O(n^2) per tick, which put the
10k-track p50 at 127 ms — over budget. Replacing the scan with a per-tick id
index brought it to 6.25 ms (about 20x). The stress mode exists precisely so
regressions like that show up as a number instead of a feeling.

## Known cost centers (measured, accepted for now)

- **Full-state snapshots.** 3.4 MB per snapshot at 10k tracks is the dominant
  wire cost at scale (at the 10 Hz broadcast rate that is ~34 MB/s per
  client). Fine at demo scale on localhost; the protocol envelope leaves room
  for a delta message kind when a real deployment needs it, and the number
  above is the justification bar it has to clear.
- **Recommendation accumulation.** Resolved recommendations stay in world
  state for the run's duration (3,678 accumulated in the 60 s stress run).
  Bounded per run and included in the serialization numbers above.
