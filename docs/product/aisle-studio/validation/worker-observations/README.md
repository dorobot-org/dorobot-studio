# Background RGB observations

In **Run → Workers**, connect to a worker and choose the background RGB period before submitting. The period cycles through 1, 2 and 5 simulated seconds. Select **Background run**. The worker captures the configured RGB image and keeps its latest frame. Use **Get snapshot** to fetch a single image. The selected foreground job continues streaming. A worker advertises supported periods at connection. The local controller independently requests the sensors it needs for navigation and balance.

**Fetch evidence** downloads the selected job, including the capture tick and SHA-256 for each background image in `background-observations.jsonl`. The last image is in `preview.rgb`, preceded by a four-byte big-endian physics tick. Background jobs save sparse operator observations. Choose **Stream run** to record a full episode for export and Replay. Nano mobile observations use the head camera. G1 standing observations use the external operator camera.

Headless and offscreen Studio both submitted overlapping real Nexus simulations, selected a two-second background period, fetched an RGB observation from the Nano, matched its checksum to an exposure in the worker archive and reopened the cancelled job. Both foreground simulations placed the red cube in 6,848 continuously supported steps and reopened all 342 exported frames in Replay. The compact 1024×768 layout keeps the sampling, cancel and fetch actions in view. [Source-bound native checks](native.json) · [Compact Run page](background-snapshot.png)

Two G1 runs of the same standing controller used a one-second and a five-second period. Each kept both feet supported for all 2,000 steps and reached the wrist target under the frontal impact. Eleven and three operator images were retained respectively, including the first observation. The largest target difference was 0.00000483 rad and the largest measured joint difference was 0.00000245 rad. [G1 samples and motor-trace comparison](g1.json)

Reproduce the G1 comparison from the Studio checkout using an installed AISLE environment and a new output directory.

```sh
.venv-aisle/bin/python tools/aisle_workbench/observation_period_check.py \
  docs/product/aisle-studio/validation/g1-frontal-impact/controller.json \
  /tmp/g1-observation-periods
```

With the worker client, pass `--observation-period-s 2` on background submission and use `observation JOB_ID output.rgb` to fetch one image. Physical multi-host throughput remains an open benchmark.
