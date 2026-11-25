# G1 standing reach with a 30-degree left waist turn

DeepSeek through Octos prepared a −0.08 rad right shoulder reach with neutral pitch and a 30° left waist turn. The controller first establishes the standing posture. The existing evaluation computes the world wrist target from the measured starting pose and scores 2,000 steps. The neutral stance remains available.

Before this qualification, the candidate reached its target and recovered in 40 previously disclosed development recipes. The paired suite reuses the latest 20 disclosed impacts, including the two sustained ankle contacts from the 20° trial. The independent qualification uses eight envelope corners and twelve sealed interior recipes, twice in separate Nexus processes. Both fresh repeats must score 20/20 on the previous criteria. All torso strikes, secondary contacts, recovery measurements, wrist and actuator traces count. [Paired manifest](matched-suite.json) · [Fresh manifest](suite.json) · [Exact controller](controller.json)
 
## Results

| Impact recipes | Reaches meeting every criterion | Loaded feet | Worst wrist error | Summed torso impulse, N·s | Measurements |
| --- | ---: | ---: | ---: | ---: | --- |
| Previously disclosed | 20/20 | 40,000/40,000 steps | 4.987 mm | 0.0073–0.1910 | [Report](matched.json) · [Raw archive](matched.zip) |
| Fresh set | 20/20 | 40,000/40,000 steps | 4.987 mm | 0.0179–0.1707 | [Report](a.json) · [Raw archive](a.zip) |
| Repeat of the same fresh set | 20/20 | 40,000/40,000 steps | 4.987 mm | 0.0179–0.1707 | [Report](b.json) · [Raw archive](b.zip) |

The two fresh repeats scored 20/20 and 20/20. The registered simulation gate is met for this controller and bounded frontal fixture. The relative reach, block trajectory, required torso contact and recovery criteria were fixed before execution. Every trial retained the motor, wrist and contact traces. [Aggregated measurements and verifier results](measurements.json)

Studio ran the DeepSeek-authored 30° reach, checked every waist target against the motor trace and opened the exported episode in Replay. Native Evidence opened both fresh archives, rejected a missing replacement and recovered the verified result after restarting Studio. [Native checks and exact sources](native.json) · [Reach evidence](reach.png) · [Evaluation view](evaluation.png)
