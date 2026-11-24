"""Bounded lateral disturbance of a floor cube during the first pickup attempt."""

import json

import numpy as np

from run_rgb_approach import evaluate_pose


class FloorSlide:
    def __init__(self, sim, cube, direction, output, floor_z, prior_lift_ticks):
        self.sim = sim
        self.cube = cube
        self.direction = {"slide-positive": 1, "slide-negative": -1}[direction]
        self.floor_z = floor_z
        self.prior_lift_ticks = prior_lift_ticks
        self.start_tick = sim.tick
        self.initial = cube.get_pos().copy()
        self.output = output
        self.first_contact = None
        self.commands = 0
        self.samples = []
        self.previous = self.initial.copy()
        self.velocity = 0.0
        self.active = True
        self.first_attempt_lift_ticks = None

    def before_step(self):
        elapsed = self.sim.tick - self.start_tick
        delta = self.direction * (self.previous[1] - self.initial[1])
        if self.active and 160 <= elapsed <= 1000 and delta < 0.095:
            velocity = self.cube.get_dofs_velocity().copy()
            velocity[1] = self.direction * 0.060
            self.sim.scene.set_body_velocity(
                0, self.cube.handles[0], velocity[:3], velocity[3:]
            )
            self.commands += 1
            return True
        return False

    def after_step(self, finger_impulses, other_support, applied):
        if self.first_attempt_lift_ticks is not None:
            return
        pos = self.cube.get_pos().copy()
        speed = float((pos[1] - self.previous[1]) / 0.01)
        touched = bool(finger_impulses)
        if touched and self.first_contact is None:
            self.first_contact = dict(
                tick=self.sim.tick,
                prior_velocity_y_m_s=self.velocity,
                displacement_y_m=float(self.previous[1] - self.initial[1]),
            )
        with self.output.open("a") as log:
            print(
                json.dumps(
                    dict(
                        tick=self.sim.tick,
                        world_m=pos.tolist(),
                        velocity_command=applied,
                        finger_impulses_ns=finger_impulses,
                        other_support=other_support,
                    )
                ),
                file=log,
            )
        self.velocity = speed
        self.previous = pos
        self.active = (
            self.active and not touched and self.sim.tick - self.start_tick <= 1200
        )

    def observe(self, command):
        if self.first_attempt_lift_ticks is not None:
            return
        base, truth = evaluate_pose(self.sim, self.cube)
        xy = command["xy"]
        row = dict(
            tick=self.sim.tick,
            xy=xy,
            base_m=base.tolist(),
            base_quat_wxyz=self.sim.robot.get_quat().tolist(),
            cameras=command["cameras"],
            intercept_xy_m=command["intercept_xy_m"],
            truth_xy_m=truth.tolist(),
            velocity_y_m_s=self.velocity,
            error_m=float(np.linalg.norm(truth - xy)) if xy is not None else None,
        )
        self.samples.append(row)

    def finish_pick(self, loaded_ticks):
        if self.first_attempt_lift_ticks is None:
            self.first_attempt_lift_ticks = loaded_ticks - self.prior_lift_ticks
            self.active = False

    def result(self):
        contact = self.first_contact or {}
        return dict(
            start_tick=self.start_tick,
            initial_world_m=self.initial.tolist(),
            floor_cube_center_z=self.floor_z,
            commands=self.commands,
            first_contact=self.first_contact,
            samples=self.samples,
            first_attempt_lift_ticks=self.first_attempt_lift_ticks,
            passed=bool(
                contact
                and (self.first_attempt_lift_ticks or 0) >= 20
                and self.direction * contact["prior_velocity_y_m_s"] > 0.003
                and self.direction * contact["displacement_y_m"] > 0.012
            ),
        )
