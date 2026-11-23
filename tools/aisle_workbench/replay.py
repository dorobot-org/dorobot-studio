"""Export synchronized robot state, commanded positions and camera images to LeRobot v2.1."""

import json
from pathlib import Path
import subprocess


def export(trial: Path, job: Path):
    import numpy as np
    import pyarrow as pa
    import pyarrow.parquet as pq
    from PIL import Image

    def read(name):
        return json.loads((trial / name).read_text())

    result, plan, record = (
        read("result.json"),
        read("controller.json"),
        read("recording.json"),
    )
    dataset = job / "dataset"
    rows, names = record["samples"], record["joints"]
    count, width = len(rows), len(names)
    transfer = result.get("kind") == "table-transfer"
    standing = result.get("kind") == "standing-inspection"
    if transfer:
        cameras = {
            name: trial / "camera" / name
            for name in ("head", "left_wrist", "right_wrist")
        }
        fps, dims = 5, (480, 360)
        expected_steps = range(20, result["physics_ticks"] + 1, 20)
        if record["cameras"] != list(cameras) or record["fps"] != fps:
            raise ValueError(
                "Camera manifest differs from the three-camera Nano recording"
            )
    elif standing:
        cameras = {"operator": trial / "camera"}
        fps, dims = 10, (480, 320)
        expected_steps = range(20, result["steps"] + 1, 20)
        if record["cameras"] != list(cameras) or record["fps"] != fps:
            raise ValueError("G1 recording requires one synchronized operator camera")
    else:
        cameras = {"operator": trial / "camera"}
        fps, dims = 20, (480, 320)
        expected_steps = range(1, 122, 5)
    if (
        not count
        or count != result.get("recorded_frames", result["frames"])
        or [r["step"] for r in rows] != list(expected_steps)
    ):
        raise ValueError("Incomplete or unsynchronized trial")
    staging = job / "episode"
    meta = staging / "meta"
    frames = staging / "data/chunk-000/episode_000000.parquet"
    for directory in (meta, frames.parent):
        directory.mkdir(parents=True)
    features, columns, stats = {}, {}, {}
    for field, source in [("observation.state", "state"), ("action", "action")]:
        values = np.asarray([row[source] for row in rows], dtype=np.float32)
        if values.shape != (count, width) or not np.isfinite(values).all():
            raise ValueError("Invalid joint sample matrix")
        columns[field] = pa.array(values.tolist(), type=pa.list_(pa.float32(), width))
        features[field] = dict(dtype="float32", shape=[width], names=names)
        stats[field] = {
            k: fn(values, axis=0).tolist()
            for k, fn in [
                ("min", np.min),
                ("max", np.max),
                ("mean", np.mean),
                ("std", np.std),
            ]
        } | {"count": [count]}
    scalars = dict(
        timestamp=[i / fps for i in range(count)],
        frame_index=range(count),
        index=range(count),
        episode_index=[0] * count,
        task_index=[0] * count,
    )
    for field, values in scalars.items():
        dtype = pa.float32() if field == "timestamp" else pa.int64()
        columns[field] = pa.array(values, type=dtype)
        stats[field] = {
            k: [float(fn(values))]
            for k, fn in [
                ("min", np.min),
                ("max", np.max),
                ("mean", np.mean),
                ("std", np.std),
            ]
        } | {"count": [count]}
        features[field] = dict(
            dtype="float32" if field == "timestamp" else "int64", shape=[1], names=None
        )
    pq.write_table(pa.table(columns), frames)
    for name, camera in cameras.items():
        cam = f"observation.images.{name}"
        video = staging / "videos/chunk-000" / cam / "episode_000000.mp4"
        video.parent.mkdir(parents=True)
        histogram = np.zeros((3, 256), dtype=np.float64)
        for i in range(count):
            with Image.open(camera / f"{i:04d}.png") as image:
                if image.size != dims or image.mode != "RGB":
                    raise ValueError(f"Invalid frame {i} in {name}")
                histogram += np.array(image.histogram()).reshape(3, 256)
        bins = np.arange(256) / 255
        mean = (histogram @ bins) / histogram.sum(axis=1)
        variance = (histogram @ (bins * bins)) / histogram.sum(axis=1) - mean**2
        low = bins[(histogram > 0).argmax(axis=1)]
        high = bins[255 - (histogram[:, ::-1] > 0).argmax(axis=1)]
        stats[cam] = {
            k: v.reshape(3, 1, 1).tolist()
            for k, v in (
                ("min", low),
                ("max", high),
                ("mean", mean),
                ("std", np.sqrt(np.maximum(variance, 0))),
            )
        } | {"count": [count]}
        subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-nostdin",
                "-framerate",
                str(fps),
                "-i",
                str(camera / "%04d.png"),
                "-frames:v",
                str(count),
                "-c:v",
                "mpeg4",
                "-q:v",
                "2",
                "-pix_fmt",
                "yuv420p",
                str(video),
            ],
            check=True,
        )
        features[cam] = dict(
            dtype="video",
            shape=[dims[1], dims[0], 3],
            names=["height", "width", "channel"],
            info={
                "video.fps": fps,
                "video.codec": "mpeg4",
                "video.pix_fmt": "yuv420p",
                "video.is_depth_map": False,
                "has_audio": False,
            },
        )

    def write(file, obj):
        (meta / file).write_text(json.dumps(obj, allow_nan=False) + "\n")

    write("episodes.jsonl", dict(episode_index=0, tasks=[plan["task"]], length=count))
    write("tasks.jsonl", dict(task_index=0, task=plan["task"]))
    write("episodes_stats.jsonl", dict(episode_index=0, stats=stats))
    write(
        "aisle.json",
        dict(
            trial=trial.name,
            physics_step_s=0.005 if standing else 0.01,
            first_physics_step=rows[0]["step"],
            **(
                dict(
                    onboard_cameras=list(cameras),
                    calibration=read("calibration.json")["cameras"],
                    optical_calibration_source=read("calibration.json").get(
                        "optical_calibration_source", "Nominal simulation optics"
                    ),
                )
                if transfer
                else dict(operator_camera="observation.images.operator")
            ),
            controller_hash=result["controller_hash"],
            controller_provider=plan["provider"],
            policy_model=plan.get("policy_model"),
            simulation_source=read("source.json"),
            model_hash=result["model_hash"],
            verdict=result["task_success"],
        ),
    )
    write(
        "info.json",
        dict(
            codebase_version="v2.1",
            robot_type={"Nano": "adora_nano", "G1": "g1"}[result["robot"]],
            fps=fps,
            total_episodes=1,
            total_frames=count,
            total_tasks=1,
            total_videos=len(cameras),
            total_chunks=1,
            chunks_size=1000,
            splits={"train": "0:1"},
            features=features,
            data_path="data/chunk-{episode_chunk:03d}/episode_{episode_index:06d}.parquet",
            video_path="videos/chunk-{episode_chunk:03d}/{video_key}/episode_{episode_index:06d}.mp4",
        ),
    )
    staging.rename(dataset)
    return dict(trial=trial.name, dataset="dataset")
