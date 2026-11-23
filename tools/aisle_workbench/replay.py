"""Export the inspection's synchronized encoder, command and operator RGB observations."""

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
    if count != result["frames"] or [r["step"] for r in rows] != list(range(1, 122, 5)):
        raise ValueError("Incomplete or unsynchronized trial")
    staging = job / "episode"
    meta = staging / "meta"
    frames = staging / "data/chunk-000/episode_000000.parquet"
    cam = "observation.images.operator"
    video = staging / "videos/chunk-000" / cam / "episode_000000.mp4"
    for p in (meta, frames.parent, video.parent):
        p.mkdir(parents=True)
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
        timestamp=[i / 20 for i in range(count)],
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
    camera = trial / "camera"
    sums = np.zeros(3)
    squares = np.zeros(3)
    low, high = np.ones(3), np.zeros(3)
    for i in range(count):
        with Image.open(camera / f"{i:04d}.png") as image:
            if image.size != (480, 320) or image.mode != "RGB":
                raise ValueError("Invalid operator frame")
            rgb = np.asarray(image, dtype=np.float64).reshape(-1, 3) / 255
            sums += rgb.sum(axis=0)
            squares += np.square(rgb).sum(axis=0)
            low, high = (
                np.minimum(low, rgb.min(axis=0)),
                np.maximum(high, rgb.max(axis=0)),
            )
    pixels = count * 480 * 320
    mean = sums / pixels
    std = np.sqrt(np.maximum(0, squares / pixels - np.square(mean)))
    stats[cam] = {
        k: v.reshape(3, 1, 1).tolist()
        for k, v in [("min", low), ("max", high), ("mean", mean), ("std", std)]
    } | {"count": [count]}
    subprocess.run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-nostdin",
            "-framerate",
            "20",
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
        shape=[320, 480, 3],
        names=["height", "width", "channel"],
        info={
            "video.fps": 20,
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
            physics_step_s=0.01,
            first_physics_step=1,
            operator_camera=cam,
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
            fps=20,
            total_episodes=1,
            total_frames=count,
            total_tasks=1,
            total_videos=1,
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
