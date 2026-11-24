//! `install.sh <dest-dir> [<binary>]`: copies the bridge and `config/tracer.json` into
//! a directory outside the dev checkout, each by an atomic rename (a running bridge
//! keeps its file), and prints the lines for `~/.config/dora-nav/env`. Run against a
//! temporary directory and a stand-in binary only.

use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("navbridge-install-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A stand-in bridge: `--help` prints the real usage line (exit `help_exit`).
fn fake_bridge(dir: &Path, tag: &str, help_exit: i32) -> PathBuf {
    let path = dir.join(format!("bridge-{tag}"));
    std::fs::write(
        &path,
        format!("#!/bin/sh\n# {tag}\necho 'usage: dorobot-nav-bridge <config.json>  (a node of a Dora dataflow; see README.md)'\nexit {help_exit}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn install(dest: &Path, binary: &Path) -> Output {
    Command::new("bash")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .arg(dest)
        .arg(binary)
        .output()
        .expect("run install.sh")
}

#[test]
fn installs_binary_and_config_and_prints_the_env_lines() {
    let dir = tmpdir("ok");
    let dest = dir.join("opt/nav-bridge");
    let out = install(&dest, &fake_bridge(&dir, "v1", 0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    let bin = dest.join("dorobot-nav-bridge");
    let cfg = dest.join("tracer.json");
    assert!(std::fs::read_to_string(&bin).unwrap().contains("# v1"));
    assert_eq!(std::fs::metadata(&bin).unwrap().permissions().mode() & 0o777, 0o755);
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/tracer.json");
    assert_eq!(std::fs::read(&cfg).unwrap(), std::fs::read(shipped).unwrap());
    assert!(stdout.contains(&format!("\nNAV_VIZ_BRIDGE={}\n", bin.display())), "{stdout}");
    assert!(stdout.contains(&format!("\nNAV_VIZ_BRIDGE_CONFIG={}\n", cfg.display())), "{stdout}");
    let mut names: Vec<String> = std::fs::read_dir(&dest).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["dorobot-nav-bridge", "tracer.json"], "no temporary files left");
    let _ = std::fs::remove_dir_all(dir);
}

/// The live bridge runs from the installed file: a new install renames a new file
/// over it, so the running copy (an open file) is not changed.
#[test]
fn reinstall_replaces_by_rename_and_leaves_an_open_copy_alone() {
    let dir = tmpdir("replace");
    let dest = dir.join("opt");
    assert!(install(&dest, &fake_bridge(&dir, "v1", 0)).status.success());
    let bin = dest.join("dorobot-nav-bridge");
    let mut running = std::fs::File::open(&bin).unwrap();
    let old_inode = running.metadata().unwrap().ino();
    let out = install(&dest, &fake_bridge(&dir, "v2", 0));
    assert!(out.status.success(), "{out:?}");
    let mut old = String::new();
    running.read_to_string(&mut old).unwrap();
    assert!(old.contains("# v1"), "the open copy is unchanged: {old}");
    assert!(std::fs::read_to_string(&bin).unwrap().contains("# v2"));
    assert_ne!(std::fs::metadata(&bin).unwrap().ino(), old_inode);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn refuses_a_path_dora_would_split_and_a_binary_that_fails_help() {
    let dir = tmpdir("refuse");
    // Whitespace, quotes, backslash, and what viz_overlay.py's PATH_OK also refuses: # $ ; ` & ( ) * ? < > | !
    // and non-ASCII.
    let bad = ["my opt", "quo'te", "dq\"uote", "back\\slash", "hash#tag", "dollar$x", "semi;colon", "tick`s", "amp&r", "par(en)", "star*", "q?", "pipe|", "bang!", "\u{4e2d}\u{6587}", "caf\u{e9}", "tab\there", "nl\nx"];
    for name in bad {
        let dest = dir.join(name);
        let out = install(&dest, &fake_bridge(&dir, "v1", 0));
        assert!(!out.status.success(), "{dest:?}: {out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("install.sh: "), "{out:?}");
        assert!(!dest.exists(), "{dest:?} not created");
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().file_name() != "bridge-v1").count(), 0);
    let dest = dir.join("opt");
    for binary in [fake_bridge(&dir, "broken", 3), dir.join("missing")] {
        let out = install(&dest, &binary);
        assert!(!out.status.success(), "{binary:?}: {out:?}");
        assert!(!dest.join("dorobot-nav-bridge").exists());
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// What `viz_overlay.py` accepts (`PATH_OK`) installs, odd but legal characters included.
#[test]
fn accepts_every_character_of_the_overlay_whitelist() {
    let dir = tmpdir("accept");
    let dest = dir.join("A-z_0.9+,:=%~/nav-bridge");
    let out = install(&dest, &fake_bridge(&dir, "v1", 0));
    assert!(out.status.success(), "{out:?}");
    assert!(dest.join("dorobot-nav-bridge").is_file());
    let _ = std::fs::remove_dir_all(dir);
}

/// The dataflow example in the README runs the installed copy, not this checkout's `target/release`.
#[test]
fn readme_dataflow_example_uses_the_installed_bridge() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();
    let node_paths: Vec<&str> = readme.lines().map(str::trim).filter(|l| l.starts_with("path:") && l.contains("dorobot-nav-bridge")).collect();
    assert!(!node_paths.is_empty(), "the README shows a dataflow node");
    for line in node_paths {
        assert!(!line.contains("target/release"), "{line}");
        assert!(line.contains("opt/nav-bridge/dorobot-nav-bridge"), "{line}");
    }
}
