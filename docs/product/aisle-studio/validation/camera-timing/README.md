# Nano camera timing and recovery

On Scene, a Nano RGB approach exposes three camera delivery settings. **On time** delivers the current packet. **Delay** repeats three observations and then delivers five packets that are 400 ms old. **Freeze** keeps delivering the same exposure. Each packet includes the head and wrist RGB images, encoders and original capture time.

The controller accepts increasing timestamps up to 250 ms old. On a rejected packet the local controller requests zero speed and the wheel guard brakes within its acceleration envelope. Fresh observations resume control. Ten consecutive missed observations at 5 Hz end the trial. Evidence records the timeout and the failed task.

| Camera delivery | Goal reached | Final cube range | Grounded steps | Withheld observations |
| --- | --- | ---: | ---: | ---: |
| On time | Yes | 0.4681 m | 460/460 | 0 |
| Delay | Yes | 0.4675 m | 620/620 | 8 |
| Freeze | No | 0.7018 m | 280/280 | 10 |

The headless test ran one DeepSeek-authored approach with identical controller bytes across all three conditions. The independent verifier opened every delivered RGB packet, checked its capture time against the control timestamp, recomputed the guarded wheel commands during the fault and checked the measured final range. All three runs recorded zero obstacle contacts. Restarting Studio preserved the frozen-camera failure. [Native verification](native-trial.json) · [All delivered packets and control traces](delivered-observations.zip)

To reproduce, run the [native verifier](../../README.md) with `--camera-faults --octos-deepseek`. This experiment exercises synchronized RGB and encoder delivery in the Nano approach controller. Independent camera failures, hardware clock synchronization and transfer-task sensor faults remain separate tests.
