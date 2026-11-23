# DeepSeek via Octos in AISLE Studio

Both providers received the same tasks on this Mac and used the same guarded Nexus worker and URDFs. The G1 tasks were entered through the Makepad UI. The second task asked for three quarters of the preceding signed movement, and each provider received its own prior measured trial. The Nano tasks exercised the same worker directly.

| Task | DeepSeek via Octos | Codex | Nexus measured displacement |
| --- | ---: | ---: | ---: |
| G1 right shoulder, −0.080 rad | 1.51 s | 6.08 s | −0.080047 rad for both |
| G1 revised to −0.060 rad | 2.13 s | 4.83 s | −0.060034 rad for both |
| Nano left shoulder, +0.120 rad | 1.42 s | 4.51 s | +0.119919 rad for both |
| Navigate and place three cubes using the inspection controller | Rejected | Rejected | No controller issued |

Times measure one CLI authoring call per provider per task, including process startup and network time. The reported models were `deepseek-v4-flash` and `gpt-6.2-astra`. This sample tests signed command generation, one evidence-informed revision and capability rejection. Both successful paths execute the same deterministic low-level commands. The full application journeys also exercised export, replay, stop and restart.

[Raw timing and outcome records](provider-comparison.json) · [Native DeepSeek Task page](octos-deepseek-task.png)
