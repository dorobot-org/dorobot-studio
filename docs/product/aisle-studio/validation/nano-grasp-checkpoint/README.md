# Train and evaluate a Nano wrist detector

In AISLE Studio, choose **Nano** on Task and prepare a floor-to-table transfer. Run it on layout 0. In Evidence, open the successful trial, choose **Export episode**, then **Train grasp detector**. Task shows the new controller revision, wrist detector and dataset identifiers. Choose a different layout on Scene and start the revised controller on Run. Evidence displays its physical verdict and detector identifier. Earlier trials and their datasets remain accessible through **Previous run**.

## What Studio learns

The exported LeRobot episode contains three synchronized onboard RGB videos, 17 encoder and action values per frame, and a carry label for each requested color. During simulation, Nexus labels an elevated cube when there is positive finger contact and no other supporting body. The physical grasp verdict separately requires opposing finger contact. At execution time, the trained model sees a left wrist RGB exposure, target color, measured gripper angle and wrist roll. Simulator contacts are available to the scorer and the offline trainer.

Training fits a class-balanced logistic classifier to colored pixel fractions in the full wrist view and six calibrated jaw regions, plus the two encoder angles. It leaves navigation, grasp geometry, release planning and actuator limits in the existing controller. The trained controller carries its weights, normalization, URDF, parent-controller hash, training report hash and dataset hash. Evidence offers training on successful, annotated teacher episodes. Older exports remain usable for Replay.

## Reproduce the three-cube comparison

Unpack the [teacher episode and complete LeRobot dataset](teacher-dataset.zip) in an empty directory. From the Studio repository root, run the following commands with the AISLE Python environment. Replace the first path with the directory where you unpacked the archive.

```sh
STUDY=/path/to/unpacked-study
.venv-aisle/bin/python tools/aisle_workbench/worker.py train-grasp \
  --robot Nano --dataset "$STUDY/dataset" --trial "$STUDY/teacher" --out "$STUDY/retrained"
.venv-aisle/bin/python tools/aisle-validation/run_rgb_transfer.py \
  --seed 1 --controller "$STUDY/retrained/controller.json" --out "$STUDY/evaluation"
```

The three-cube teacher produced 1,187 frames on layout 0. Across the three requested colors this yielded 518 positive and 3,043 negative examples. On the training data, the classifier detected all 518 positives and produced 92 false positives. Unpacking the archive and running the trainer again reproduced the three-cube controller byte for byte on the test machine. [Training report](training.json) · [Portable controller](controller.json)

On layout 1 the original and trained controllers both placed all three cubes in 29,302 physics ticks, with continuous wheel support and zero furniture contact. Their motor journals were identical. All 15 online classifier decisions replayed from the saved wrist PNGs and encoder readings. Eleven positive and four negative predictions matched the independent physical labels. This one paired layout demonstrates reproducibility. It provides no measured improvement in transfer success. [Paired results](matched.json) · [Raw trials and exposures](matched-trials.zip)

## Native Studio

The offscreen DeepSeek/Octos journey trained a separate detector from a red-cube episode on layout 0. On layout 1 the Nano placed red in 7,608 physics ticks. Three held observations and one empty observation agreed with the physical sensor labels and replayed from the exact wrist exposures. After restart Studio restored the same detector and verdict. [Native checks](native.json) · [Task](task.png) · [Evidence](evidence.png)

The 1024 × 768 native headless Codex journey trained on three cubes. All three settled on layout 1, with 29,302 supported physics ticks. The 15 online wrist decisions replayed exactly and matched all 15 physical labels. Studio restored the trained Codex-authored revision after restart. [Headless and offscreen receipts](native.json)
