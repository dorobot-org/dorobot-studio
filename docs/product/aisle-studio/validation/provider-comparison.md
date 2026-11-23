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

The subsequent floating-base trial exercised a separate Nano wheel controller. Both providers approached the red cube in Nexus and reproduced the same command and measurement trace. [RGB approach comparison, live view and replay](nano-rgb-approach.md)

The grounded Nano transfer capability subsequently exercised both providers on a complete floor-to-table task. [Task setup, matching criteria and measured outcomes](nano-table-transfer.md)

## Five matched requests and a retry from measured feedback

The following authoring calls used the same Workbench validation code and task text. The provider order alternated across cases. Each provider received one attempt per case. The revision received per-cube verdicts and the last onboard wrist and stereo readings from the same retained failure on layout 2.

| Request | DeepSeek via Octos | Codex | Accepted program or outcome |
| --- | ---: | ---: | --- |
| Place green, blue, then red | 2.97 s | 6.70 s | Both preserved the requested sequence |
| Approach blue to 0.52 m | 2.21 s | 5.98 s | Both selected blue and 0.52 m |
| G1 standing left shoulder +0.10 rad | 1.89 s | 4.99 s | Both selected standing and the correct joint and sign |
| G1 walk and open a door | 2.14 s | 6.39 s | Both rejected the unsupported capability |
| Retry the cube that failed | 1.56 s | 5.14 s | Both identified blue from the recorded verdict |

Each provider met all five expectations. Median authoring time was **2.14 s for DeepSeek and 5.98 s for Codex** on this Mac. The timed interval includes CLI startup and model-service latency. This small set exercises constrained task authoring and rejection.

The two independently prepared blue-only retry artifacts were then executed from the identical initial layout in Nexus. Each placed blue 120 mm inside the rim, maintained wheel contact on all 7,268 physics steps and produced byte-identical motor journals. This tests the selected retry subtask from a fresh scene. The three-cube holdout evaluates full episodes separately. The model chooses the task parameters, while local camera perception, navigation and grasp code drive the simulated robot. [Requests, model identities, timing, feedback and both retry receipts](paired-authoring-requests.json)


## Hand an existing Codex run to DeepSeek

In the native Task page, **Revise with DeepSeek** can use a selected Codex, reference or DeepSeek trial. **Revise with Codex** uses the same measured evidence and validation rules. The recorded parent hash connects the selected run to the new controller.

In the headless G1 inspection, Codex requested −0.080 rad and Nexus measured −0.079959 rad. DeepSeek received that result and the instruction “Repeat the preceding inspection at three quarters of its signed displacement.” It selected the same right shoulder, requested −0.060 rad and Nexus measured −0.059967 rad. Restarting Studio restored the scored G1 trial. [Requests, controller lineage, measurements and native build](provider-handoff.json)

To repeat this handoff, follow the workbench setup and run the native verifier with `--handoff`. To run a new simulation using DeepSeek alone, author it with **DeepSeek via Octos** and revise with **Revise with DeepSeek**.
