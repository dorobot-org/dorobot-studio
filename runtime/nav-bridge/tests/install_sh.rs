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
    for dest in [dir.join("my opt"), dir.join("quo'te"), dir.join("back\\slash")] {
        let out = install(&dest, &fake_bridge(&dir, "v1", 0));
        assert!(!out.status.success(), "{dest:?}: {out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("install.sh: "), "{out:?}");
        assert!(!dest.exists(), "{dest:?} not created");
    }
    let dest = dir.join("opt");
    for binary in [fake_bridge(&dir, "broken", 3), dir.join("missing")] {
        let out = install(&dest, &binary);
        assert!(!out.status.success(), "{binary:?}: {out:?}");
        assert!(!dest.join("dorobot-nav-bridge").exists());
    }
    let _ = std::fs::remove_dir_all(dir);
}
