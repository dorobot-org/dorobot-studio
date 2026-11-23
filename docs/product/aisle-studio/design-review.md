# Design review · inspection workbench

[Selected ImageGen board](workbench-design.png) · [first concept](workbench-concept-v1.png) · [requirements and scoring criteria](PRD.md)

Codex visual review of the generated design, 2026-11-23. The score applies to the four-screen design at desktop size. Implementation screenshots and headless interaction checks are separate evidence.

| Criterion | Points / 10 | Observation |
| --- | ---: | --- |
| Studio shell consistency | 9 | Shared toolbar, segmented navigation and restrained color fit the existing shell. Sidebar labels differ slightly from shipping names. |
| Page identity | 10 | Heading and selected tab agree on all four pages. |
| Next action | 8 | Inspect, start and open are clear. Three controller buttons compete on Task. Use two authoring actions in implementation. |
| Robot identity | 10 | Named robot, appropriate silhouette and model-specific joint counts. |
| Simulation context | 10 | Engine and fixed-base fixture both visible. |
| State visibility | 8 | Run progress and stop affordance present. Error and idle states require implementation checks. |
| Task-result separation | 9 | Outcome and measurements sit together. Include completed-process status in the implementation. |
| Readability | 10 | Strong contrast, left-aligned labels and legible primary controls. |
| Grouping and spacing | 10 | Operator view and decision controls remain distinct. |
| Artifact discoverability | 8 | Clear folder action. List the actual artifact names in the implementation. |
| Total | **92** | Meets the 90-point design target in this internal review. |

The first concept depicted the wrong engine and generic robot shapes. The selected revision used the two actual Nexus operator-camera renders as references. Chart values in the design are illustrative. Runtime values come from saved measurements.

Built-in ImageGen prompt set: generate a four-screen Task, Scene, Run and Evidence design board in the current DoRobot Studio visual language with a controller draft, model inspector, camera view and outcome timeline. Revise it to use the supplied Nano and G1 simulation silhouettes, highlight AISLE Studio, label the engine AISLE / Nexus and show the fixed-base fixture, actual model counts, requested 0.100 rad, measured 0.09993 rad and 25 observations.
