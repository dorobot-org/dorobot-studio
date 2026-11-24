"""Check freshness of the left-wrist RGB stream while the Nano picks an object."""

from collections import Counter, deque
import hashlib
import json

from PIL import Image

from run_rgb_approach import MAX_MISSING_OBSERVATIONS, timing_fault


class WristTiming:
    interval_ticks = 20

    def __init__(self, fixture, out, retain_images):
        if fixture not in ("normal", "intermittent", "disconnected"):
            raise ValueError("Unknown wrist RGB timing fixture")
        self.fixture = fixture
        self.out = out
        self.retain_images = retain_images
        self.recent = deque(maxlen=3)
        self.frozen = None
        self.next_tick = 0
        self.sequence = 0
        self.last_accepted_ns = -1
        self.accepted_rgb = None
        self.missed = 0
        self.rejected = Counter()
        self.recoveries = 0
        self.timed_out = False

    def accept(self, tick, capture, synchronized=False):
        self.next_tick = tick + self.interval_ticks
        received_ns = tick * 10_000_000
        rgb = capture().copy()
        digest = hashlib.sha256(rgb.tobytes()).hexdigest()
        exposure = (tick, rgb, digest)
        if self.sequence < 15 and self.retain_images:
            folder = self.out / "wrist-exposures"
            folder.mkdir(exist_ok=True)
            Image.fromarray(rgb).save(folder / f"{tick:07d}.png", compress_level=1)
        self.recent.append(exposure)
        if self.sequence == 3:
            self.frozen = exposure
        if self.fixture != "normal" and self.sequence >= 4:
            if self.fixture == "disconnected" or self.sequence < 7:
                exposure = self.frozen
            elif self.sequence < 12:
                exposure = self.recent[0]
        capture_tick, delivered_rgb, delivered_digest = exposure
        captured_ns = capture_tick * 10_000_000
        reason = timing_fault(captured_ns, received_ns, self.last_accepted_ns)
        if reason is None and synchronized and captured_ns != received_ns:
            reason = "unsynchronized wrist packet"
        if reason:
            self.missed += 1
            self.rejected[reason] += 1
        else:
            self.last_accepted_ns = captured_ns
            self.accepted_rgb = delivered_rgb
            self.recoveries += int(self.missed > 0)
            self.missed = 0
        with (self.out / "wrist-delivery.jsonl").open("a") as log:
            print(
                json.dumps(
                    dict(
                        tick=tick,
                        capture_tick=capture_tick,
                        capture_ns=captured_ns,
                        receive_ns=received_ns,
                        source_sha256=digest,
                        delivered_sha256=delivered_digest,
                        fault=reason,
                    )
                ),
                file=log,
            )
        self.sequence += 1
        if self.missed >= MAX_MISSING_OBSERVATIONS:
            self.timed_out = True
            raise ValueError("Left wrist RGB timed out during pick")
        return reason is None

    def result(self):
        return dict(
            camera="left_wrist",
            phase="pick",
            fixture=self.fixture,
            received=self.sequence,
            rejected=dict(self.rejected),
            recoveries=self.recoveries,
            timed_out=self.timed_out,
        )
