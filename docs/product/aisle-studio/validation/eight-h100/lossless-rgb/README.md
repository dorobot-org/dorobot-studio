# Receive background Nano images from a remote fleet

On **Run**, connect to the configured worker and choose **Stream run** for the foreground Nano. For the other workers select **Wall time**, **RGB / 0.5 s** and **Background run**. Select a running background job and choose **Snapshot** to inspect its latest head-camera exposure. The operator can inspect a background job while the foreground simulation continues.

The worker can now send its lease status and latest background image in a single request. The client asks for lossless zlib compression and can send its last received tick so the worker can omit duplicate images. It validates the dimensions and decoded size before displaying or hashing the pixels. Existing clients can still request the original raw RGB representation. Updated clients can read an older worker’s snapshot and query its status separately.

## Same eight-H100 workload

One H100 NVL ran the foreground stream and recorded the three onboard cameras. Seven more GPUs each ran one independent Nano and published head RGB on a 0.5-second wall clock. Each trial reran the same saved controller on the same room layout. All sixteen executions produced motor journals byte-identical to their earlier counterparts. The earlier background protocol used separate status and uncompressed RGB requests. Both configurations streamed fresh 480 × 360 head-camera frames to the native Makepad viewport.

| Controller author | Eight-way overlap | Foreground source / Mac viewport | Previous seven receivers | Updated seven receivers | Empty seconds per updated receiver | Image payload reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Codex | 102.07 s | 17.95 / 17.48 FPS | 0.70–0.71 Hz | 1.92–1.97 Hz | 0 / 102 | 95.87% |
| DeepSeek via Octos | 101.17 s | 17.96 / 17.69 FPS | 1.08–1.10 Hz | 1.97–2.00 Hz | 0–1 / 101 | 95.88% |

All fourteen background receivers averaged at least 1 Hz. Across both runs, one receiver had one complete second without a fresh image. Both native foreground rates remained below the 25 FPS target. All sixteen single-red-cube transfers passed.

The payload column compares the actual base64 image characters with the base64 size for the same received RGB images before compression. It excludes SSH and JSON headers. The empty-second count is calculated separately for each of the seven feeds using complete, non-overlapping one-second windows on the Mac. Each verifier matches the tick and pixel digest of every received image to the original worker publication. Both foreground datasets reopened with 347 synchronized frames per camera. The authors prepared their programs before these runs.

## Reproduce the audit

Run the following from the repository root using the Studio Python environment. The dataset option decodes all six new recordings with FFprobe and matches video timestamps to the 17-channel action and encoder rows. The worker archive includes the original camera receipts, source timestamps, GPU samples, motor journals and dataset files.

```sh
.venv-aisle/bin/python docs/product/aisle-studio/validation/eight-h100/lossless-rgb/audit_transport.py --datasets
```

Use the existing eight-worker runner with `--background-clock wall --background-period-s 0.5` to take a new measurement. Run each cohort separately. [Worker registration and benchmark commands](../README.md#workload-and-measurement)

[Per-worker results and complete-second counts](comparison.json) · [Measured archive](measurements.zip) · [Previous wall-clock measurements](../wall-clock/README.md)

## Native Snapshot and local Mac capacity

On the 1024 × 768 headless Run page, **Snapshot** retrieved a real Nano frame at tick 661. Its decoded pixels matched the worker’s publication. **Cancel job** stopped the selected background lease. The foreground transfer continued, and its 342-frame export opened in Replay after restart. A separate completed H100 job returned 32,332 base64 payload bytes for the same image that a legacy request returned in 691,200 bytes. A repeat request for that tick returned status with zero image bytes. The older deployed worker returned its original raw image to the updated client. [Snapshot pixels, source record and compatibility receipts](native-snapshot.zip)

On the M3 Max, one native offscreen foreground and nine background Nano sessions completed with DeepSeek through Octos. The foreground averaged 28.61 FPS, with 1 of 93 complete seconds below 25 FPS. The nine receivers averaged 1.997–2.006 Hz. All 10 red-cube transfers passed. [Mac results](mac-ten.json) · [Native draws, decoded-image digests and motor journals](mac-ten.zip)

## H100 source camera timing

A separate diagnostic replayed the first 320 steps of the same motor journal on an idle H100 NVL, requesting one real 480 × 360 head-camera exposure per step. After 64 warm-up steps, the median camera render and readback took 32.02 ms, physics 6.08 ms, independent scoring 7.54 ms and scene synchronization 0.24 ms. The current rigid contact impulse query accounted for 6.14 ms of scoring. The full measured step averaged 46.30 ms. Sustaining 25 new frames per second permits 40 ms per frame. These are sequential, host-clock measurements of one section of the trajectory. The measured interval covers local simulation, scoring, synchronization and RGB readback on the worker. [Timing program](profile_source.py) · [All 256 step records](source-stages.json.gz)
