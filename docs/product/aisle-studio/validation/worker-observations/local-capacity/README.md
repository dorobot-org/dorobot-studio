# One foreground transfer with three background sessions

An Apple M3 Max with 128 GiB RAM ran four concurrent Studio worker sessions. Each issued its own DeepSeek request through Octos, then used the AISLE Nexus backend to transfer the red floor cube onto the coffee table on layout 0. One worker recorded and streamed. The other three ran with an observation period of one simulated second. Each simulation lasted 6,848 physics steps with continuous wheel support, a settled cube and a passing motor journal.

| Measured on this Mac | One foreground | One foreground + three background |
| --- | ---: | ---: |
| Successful sessions | 1/1 | 4/4 |
| Time from first author request to all verdicts | 71.15 s | 73.95 s |
| Time including verified archive downloads and foreground dataset export | 74.64 s | 77.51 s |
| Distinct foreground images delivered per wall second in the common lease window | 5.39 | 5.03 |
| Images in the exported head, left wrist and right wrist videos | 342 each | 342 each |

All four leases overlapped for 64.73 seconds. For a stricter overlap window, take the latest first positive physics progress and earliest last running progress from the sampled status of all four workers. Over those 57.66 seconds, the client observed 289 distinct foreground updates (5.01 FPS) and 65, 66 and 66 background images (1.13 FPS per session). Frame rates use the intervals between the first and last distinct worker ticks seen by each client. Authoring took 1.99–2.67 seconds across the four concurrent requests. Each full background trial delivered 68 RGB observations. The foreground exported all three synchronized onboard cameras.

These are two runs on one host with the same layout and program. They establish a tested four-session configuration at roughly five foreground FPS. The 25–30 FPS target, maximum session count and physical multi-host throughput remain open. The foreground client measured receipt of decoded RGB bytes via the worker API. Native Makepad display cadence was measured separately. [Earlier Makepad timing samples](../../worker-leases/cadence.json)

## Reproduce

Set up the [Studio runtime](../../../README.md#run-from-a-studio-source-checkout), including the Octos DeepSeek profile and FFmpeg. Use the [exact Studio source from these runs](https://github.com/dorobot-org/dorobot-studio/tree/67fa8a41fb762f67009963baf1389e7ad28f6fce). The script uses the configured AISLE Python, submits an independent author request per session, retrieves frames through the normal fleet RPC, downloads and verifies every archive, then exports and counts the three foreground videos. Run from that Studio checkout and point `bench` to this directory’s [measurement script](measure.py). Both output directories must be new.

```sh
scratch=$(mktemp -d)
for count in 1 4; do
  .venv-aisle/bin/python "$bench/measure.py" "$PWD" "$scratch/run-$count" --sessions "$count"
done
```

The compressed reports include worker identity and version, per-job lifecycle, source ticks, local monotonic receive times, RGB hashes, archived evidence digests, task results and export frame counts. [One session](1-sessions.json.gz) · [Four sessions](4-sessions.json.gz)
