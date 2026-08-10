"""CGWindowID of the running ui_mock window, for occlusion-proof capture."""

import Quartz

OPTS = Quartz.kCGWindowListExcludeDesktopElements | Quartz.kCGWindowListOptionOnScreenOnly

for w in Quartz.CGWindowListCopyWindowInfo(OPTS, Quartz.kCGNullWindowID) or []:
    if "ui_mock" in (w.get("kCGWindowOwnerName") or ""):
        bounds = w.get("kCGWindowBounds") or {}
        # Skip the tiny helper surfaces the window server also reports.
        if (bounds.get("Height") or 0) > 200:
            print(int(w["kCGWindowNumber"]))
            break
