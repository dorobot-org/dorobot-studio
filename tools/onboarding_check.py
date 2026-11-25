#!/usr/bin/env python3
"""OC-spec: 结构性自检，保证 docs/ONBOARDING.md 的说法与代码一致。

用法：
    python3 tools/onboarding_check.py          # 表格 + 退出码
    python3 tools/onboarding_check.py -v       # 附带证据

退出码：0 = 全部通过；1 = 至少一项失败。

设计原则：每条检查只断言"文档依赖的事实"，不断言代码风格。
代码演进导致文档过期时，这里必须变红——红就是让文档改，不是让检查放过。
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOC = ROOT / "docs/ONBOARDING.md"
results: list[tuple[str, str, bool, str]] = []


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def nlines(rel: str) -> int:
    return len(read(rel).splitlines())


def source_files() -> list[Path]:
    """统计口径：排除任何 vendor/target 目录与顶层 examples/（保留 dorobot-flex/examples）。"""
    out: list[Path] = []
    for path in ROOT.rglob("*.rs"):
        rel = path.relative_to(ROOT)
        if "vendor" in rel.parts or "target" in rel.parts:
            continue
        if rel.parts[0] == "examples":
            continue
        out.append(path)
    return out


def check(cid: str, spec: str, fn) -> None:
    try:
        ok, evidence = fn()
    except Exception as exc:  # 检查本身坏了也要报红
        ok, evidence = False, f"检查异常: {exc}"
    results.append((cid, spec, ok, evidence))


# ---------------------------------------------------------------- 骨架
def oc01() -> tuple[bool, str]:
    block = read("Cargo.toml").split("members = [")[1].split("]")[0]
    members = set(re.findall(r'"([^"]+)"', block))
    want = {
        "crates/dorobot-ux",
        "crates/dorobot-types",
        "crates/dorobot-dora-bridge",
        "dorobot-flex",
        "crates/dorobot-studio-core",
        "crates/dorobot-studio-plugin",
        "plugins/collection",
        "plugins/control",
        "plugins/policy",
        "plugins/aisle",
    }
    return members == want, f"{len(members)} 个成员，集合一致={members == want}"


def oc02() -> tuple[bool, str]:
    text = read("dorobot-flex/src/plugins.rs")
    ids = re.findall(r"register\(Box::new\((dorobot_plugin_\w+)::\w+Plugin\)\)", text)
    return len(ids) == 4, f"注册 {len(ids)} 个内置插件: {', '.join(ids)}"


def oc03() -> tuple[bool, str]:
    counts = {
        name: len(re.findall(r"PanelDescriptor\s*\{", read(f"plugins/{name}/src/lib.rs")))
        for name in ("control", "collection", "policy", "aisle")
    }
    total = sum(counts.values())
    return total == 11, (
        f"control={counts['control']} collection={counts['collection']} "
        f"policy={counts['policy']} aisle={counts['aisle']} 共 {total}"
    )


def oc04() -> tuple[bool, str]:
    found = {}
    for name in ("control", "collection", "policy", "aisle"):
        chunks = read(f"plugins/{name}/src/lib.rs").split("PanelDescriptor {")[1:]
        for chunk in chunks:
            head = chunk[:400]
            if "legacy_screen: None" in head:
                pid = re.search(r'id:\s*"([^"]+)"', head)
                if pid:
                    found[pid.group(1)] = name
    want = {
        "nexus_setup",
        "hardware_live_mirror",
        "nexus_train",
        "nexus_rollout",
        "aisle_task",
        "aisle_scene",
        "aisle_run",
        "aisle_evidence",
    }
    return set(found) == want, f"{len(found)} 个无 legacy 面板: {sorted(found)}"


def oc05() -> tuple[bool, str]:
    block = re.search(r"pub enum Screen \{(.*?)\n\}", read("crates/dorobot-studio-core/src/api/mod.rs"), re.S)
    variants = re.findall(r"^\s{4}([A-Z]\w*),", block.group(1), re.M)
    want = ["Library", "Hardware", "Record", "Play", "Live", "Eval"]
    return variants == want, f"{len(variants)} 个变体: {variants}"


# ---------------------------------------------------------------- 契约
def oc06() -> tuple[bool, str]:
    block = re.search(
        r"pub trait Backend \{(.*?)\n\}\n",
        read("crates/dorobot-studio-core/src/api/mod.rs"),
        re.S,
    ).group(1)
    mutators = re.findall(r"fn (\w+)\(&mut self", block)
    return mutators == ["dispatch", "poll"], f"&mut self 方法: {mutators}"


def oc07() -> tuple[bool, str]:
    block = re.search(
        r"pub trait PluginPanel \{(.*?)\n\}", read("crates/dorobot-studio-plugin/src/lib.rs"), re.S
    ).group(1)
    names = re.findall(r"fn (\w+)\(", block)
    want = ["root", "sync", "handle_actions", "on_disable", "visible_in"]
    return names == want, f"{len(names)} 个方法: {names}"


def oc08() -> tuple[bool, str]:
    text = read("crates/dorobot-studio-plugin/src/lib.rs")
    ok = re.search(r"if active \{\s*panel\.instance\.sync\(cx, context\);", text) is not None
    return ok, "sync 只在 active 分支内调用" if ok else "未找到 active 门控"


def oc09() -> tuple[bool, str]:
    core = read("crates/dorobot-studio-core/Cargo.toml")
    plugin = read("crates/dorobot-studio-plugin/Cargo.toml")
    ok = (
        'default = ["video"]' in core
        and 'video = ["ffmpeg-next"]' in core
        and 'dorobot-studio-core = { path = "../dorobot-studio-core", default-features = false }' in plugin
    )
    return ok, "core 默认 video；插件以 default-features=false 引入"


def oc10() -> tuple[bool, str]:
    text = read("Cargo.toml")
    pins = {
        "makepad rev": "44d8220f12acfa46157019267e119e0866dd6b05" in text,
        "ffmpeg =8.0.0": 'ffmpeg-next = "=8.0.0"' in text,
        "bitflags patch": "bitflags = { git = " in text,
    }
    return all(pins.values()), ", ".join(f"{k}={v}" for k, v in pins.items())


def oc11() -> tuple[bool, str]:
    rt = read("runtime/Cargo.toml")
    ok = (
        "[workspace]" in rt
        and 'edition = "2024"' in rt
        and (ROOT / "runtime/Cargo.lock").is_file()
        and (ROOT / "Cargo.lock").is_file()
    )
    return ok, "runtime 独立 workspace + 两份 Cargo.lock"


def oc12() -> tuple[bool, str]:
    ci = read(".github/workflows/ci.yml")
    ok = "1.95.0" in ci and "cargo test --locked --workspace --no-default-features" in ci
    return ok, "CI: rust 1.95.0 + workspace 无视频测试"


# ---------------------------------------------------------------- 规模
def oc13() -> tuple[bool, str]:
    files = source_files()
    total = sum(len(p.read_text(encoding="utf-8").splitlines()) for p in files)
    tests = sum(
        len(re.findall(r"#\[test\]", p.read_text(encoding="utf-8")))
        for p in files
    )
    ok = 80 <= len(files) <= 110 and 30_000 <= total <= 40_000 and tests >= 120
    return ok, f"{len(files)} 文件 / {total} 行 / {tests} 个 #[test]"


# ---------------------------------------------------------------- 文档依赖的路径
def oc14() -> tuple[bool, str]:
    need = [
        "docs/PLUGINS.md",
        "docs/USER_GUIDE.md",
        "docs/ux/functional-review.md",
        "tools/ux/capture_matrix.py",
        "tools/run_jit.sh",
        "docs/superpowers/plans/2026-11-17-rust-plugin-workbench.md",
    ]
    missing = [p for p in need if not (ROOT / p).exists()]
    return not missing, "全部存在" if not missing else f"缺失 {missing}"


def oc15() -> tuple[bool, str]:
    catalog = ROOT / "dorobot-flex/resources/i18n/zh-CN.json"
    inc = 'include_str!("../../../dorobot-flex/resources/i18n/zh-CN.json")' in read(
        "crates/dorobot-studio-core/src/i18n.rs"
    )
    return catalog.is_file() and inc, "i18n 目录被 core include_str! 编译进来"


def oc16() -> tuple[bool, str]:
    ok = 'a == "--demo"' in read("dorobot-flex/src/bin/dorobot-flex.rs")
    return ok, "--demo 由 make_backend 识别"


def oc17() -> tuple[bool, str]:
    text = read("dorobot-flex/tests/plugins.rs")
    guards = [
        "every_builtin_factory_and_toolbox_has_no_script_errors",
        "toolbox_has_one_entry_per_plugin_instead_of_one_per_panel",
        "disconnected_operate_panel_respects_the_hosts_setup_gate",
        "plugin_panel_named_play_cannot_shadow_the_hosts_replay_tab",
        "collection_factory_creates_independent_real_views_and_routes_record_stop_save",
    ]
    missing = [g for g in guards if g not in text]
    return not missing, "5 个护栏测试齐全" if not missing else f"缺失 {missing}"


# ---------------------------------------------------------------- 文档自身是否过期
def doc_table_row(path_fragment: str) -> int | None:
    """在文档的阅读清单表格里，取路径单元格**后面**那一格（行数）。

    表格形如：| 1 | `path` | 570 | ...
    只看路径格之后的格子，避免误取序号列。
    """
    for line in read("docs/ONBOARDING.md").splitlines():
        if path_fragment not in line or not line.lstrip().startswith("|"):
            continue
        cells = [c.strip() for c in line.split("|")]
        for i, cell in enumerate(cells):
            if path_fragment in cell:
                for nxt in cells[i + 1 :]:
                    m = re.fullmatch(r"~?(\d+)", nxt)
                    if m:
                        return int(m.group(1))
    return None


def oc18() -> tuple[bool, str]:
    actual = nlines("crates/dorobot-studio-core/src/api/mod.rs")
    claimed = doc_table_row("api/mod.rs")
    return claimed == actual, f"文档={claimed} 实际={actual}"


def oc19() -> tuple[bool, str]:
    actual = nlines("crates/dorobot-studio-plugin/src/lib.rs")
    claimed = doc_table_row("dorobot-studio-plugin/src/lib.rs")
    return claimed == actual, f"文档={claimed} 实际={actual}"


def oc20() -> tuple[bool, str]:
    actual = nlines("dorobot-flex/tests/plugins.rs")
    claimed = doc_table_row("dorobot-flex/tests/plugins.rs")
    return claimed == actual, f"文档={claimed} 实际={actual}"


def oc21() -> tuple[bool, str]:
    actual = nlines("plugins/collection/src/lib.rs")
    claimed = doc_table_row("plugins/collection/src/lib.rs")
    # 文档用 "~N" 表示近似值，容差 10%
    ok = claimed is not None and abs(claimed - actual) <= max(5, actual // 10)
    return ok, f"文档≈{claimed} 实际={actual}"


def oc22() -> tuple[bool, str]:
    doc = read("docs/ONBOARDING.md")
    files = source_files()
    total = sum(len(p.read_text(encoding="utf-8").splitlines()) for p in files)
    tests = sum(len(re.findall(r"#\[test\]", p.read_text(encoding="utf-8"))) for p in files)
    claims = {
        f"{len(files)} 个 Rust 文件": f"{len(files)} 个 Rust 文件" in doc,
        f"{total / 10000:.1f} 万行": f"{total / 10000:.1f} 万行" in doc,
        f"{tests} 个 `#[test]`": f"{tests} 个 `#[test]`" in doc,
    }
    return all(claims.values()), ", ".join(f"{k}={v}" for k, v in claims.items())


checks = [
    ("OC-01", "根 workspace 有 10 个成员且集合固定", oc01),
    ("OC-02", "组合根注册 4 个内置插件", oc02),
    ("OC-03", "面板总数为 11（4+1+2+4）", oc03),
    ("OC-04", "legacy_screen=None 的面板恰为 8 个", oc04),
    ("OC-05", "Screen 有 6 个变体", oc05),
    ("OC-06", "Backend 只有 dispatch/poll 两个 &mut 方法", oc06),
    ("OC-07", "PluginPanel 有 5 个方法", oc07),
    ("OC-08", "Registry 只对可见面板调用 sync", oc08),
    ("OC-09", "视频是 core 的可选特性，插件默认关闭", oc09),
    ("OC-10", "Makepad / ffmpeg / bitflags 三者锁定", oc10),
    ("OC-11", "runtime 是独立 workspace（edition 2024）", oc11),
    ("OC-12", "CI 用 1.95.0 跑无视频 workspace 测试", oc12),
    ("OC-13", "规模在 80–110 文件 / 3–4 万行 / ≥120 测试", oc13),
    ("OC-14", "文档引用的 6 个路径都存在", oc14),
    ("OC-15", "i18n 目录被编译进 core", oc15),
    ("OC-16", "--demo 由 make_backend 识别", oc16),
    ("OC-17", "5 个护栏测试都在", oc17),
    ("OC-18", "文档中 api/mod.rs 行数未过期", oc18),
    ("OC-19", "文档中 plugin/src/lib.rs 行数未过期", oc19),
    ("OC-20", "文档中 tests/plugins.rs 行数未过期", oc20),
    ("OC-21", "文档中 collection 切片行数未过期（±10%）", oc21),
    ("OC-22", "文档中规模数字未过期", oc22),
]

for cid, spec, fn in checks:
    check(cid, spec, fn)

verbose = "-v" in sys.argv
width = max(len(s) for _, s, _, _ in results)
print(f"{'ID':<6} {'检查项':<{width}} 结果")
print("-" * (width + 14))
for cid, spec, ok, evidence in results:
    mark = "PASS" if ok else "FAIL"
    print(f"{cid:<6} {spec:<{width}} {mark}")
    if verbose or not ok:
        print(f"       └─ {evidence}")

failed = [r for r in results if not r[2]]
print("-" * (width + 14))
print(f"{len(results) - len(failed)}/{len(results)} 通过")
sys.exit(1 if failed else 0)
