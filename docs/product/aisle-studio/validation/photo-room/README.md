# Draft a room from a calibrated photograph

On Task, prepare a Nano living-room controller. Open **Scene → Room survey** and enter the absolute path to the [photo input](photo-input.json). Under **Draft from reference photo**, choose **DeepSeek** or **Codex**. Review the proposed footprint over the image and check its position, width, depth and height. Choose **Use this room** to select it for simulation, or **Discard draft** to keep the current room. Run the controller and open Evidence. [Draft review](draft.png) · [Selected room](applied.png)

The input consists of a reference image, a description and at least five surveyed floor marks. Coordinates use the Nano's initial base frame, with +X forward and +Y left. Supply an upright, undistorted image with approximately square pixels and a centered principal point. Studio infers focal length from the two room axes across the floor marks. Supply the calibrated focal length in pixels as `focal_px` if one axis has a vanishing point at infinity.

Studio searches the RGB image for connected colored regions with a measurable six-corner silhouette. It fits room-aligned upright cuboids by projecting their corners into the image. The provider receives the description, candidate IDs, RGB values, bounds and fitted geometry, and selects and names the requested objects. The generated room inherits the existing collision and scale checks. A scene with occluded corners, complex furniture, low contrast or uncertain scale can require a manual survey. [Surveyed geometry workflow](../room-survey/README.md)

## Native measurements

The reference is a Nexus-rendered living room containing a teal ottoman. Only the six floor marks from the independent survey were included in the photo input. The operator requested the foreground ottoman. Both providers selected it and Studio measured the same box.

| Measurement | Image fit | Independent survey |
| --- | ---: | ---: |
| Floor center X | 1.273 m | 1.280 m |
| Floor center Y | −0.446 m | −0.450 m |
| Width along X | 0.395 m | 0.400 m |
| Depth along Y | 0.304 m | 0.310 m |
| Height | 0.329 m | 0.330 m |

The maximum image vertex residual was 0.45 px. A blank-image regression produced zero candidates using the same marks. The 1440 × 900 offscreen DeepSeek/Octos and 1024 × 768 headless Codex journeys each presented a draft before selection, applied it, verified the Nexus body center to within 0.01 mm of the accepted revision and completed the red-cube transfer with all 6,848 ticks supported. Both restored the accepted room and its measured trial after restart. These trials cover one rendered photograph and one box-shaped obstacle. [Native measurements, provider responses and physical results](native.json) · [Trial evidence](evidence.png)
