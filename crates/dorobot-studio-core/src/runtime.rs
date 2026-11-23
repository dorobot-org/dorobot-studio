//! Installed resources are read-only; sessions always live outside the app bundle.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn bundle_resources(exe: &Path) -> Option<PathBuf> {
    let contents = exe.parent()?.parent()?;
    let resources = contents.join("Resources");
    (contents.file_name()? == "Contents" && resources.join("release.json").is_file())
        .then_some(resources)
}

pub fn resources() -> Option<PathBuf> {
    std::env::var_os("DOROBOT_RESOURCES")
        .map(PathBuf::from)
        .or_else(|| bundle_resources(&std::env::current_exe().ok()?))
}

pub fn root() -> PathBuf {
    std::env::var_os("DOROBOT_STUDIO_SOURCE_ROOT")
        .map(PathBuf::from)
        .or_else(|| resources().map(|p| p.join("studio")))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

pub fn user_data() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::env::var_os("DOROBOT_STUDIO_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                home.join("Library/Application Support/DoRobot Studio")
            } else {
                home.join(".local/share/dorobot-studio")
            }
        })
}

pub fn runs() -> PathBuf {
    if resources().is_some() {
        user_data().join("runs")
    } else {
        root().join("tools/aisle-validation/runs")
    }
}

pub fn datasets() -> PathBuf {
    if resources().is_some() {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("DoRobot/Recordings")
    } else {
        root().join("datasets")
    }
}

pub fn native() -> PathBuf {
    std::env::var_os("DOROBOT_NATIVE_RUNTIME")
        .map(PathBuf::from)
        .or_else(|| resources().map(|p| p.join("runtime/bin/dorobot-nano-runtime")))
        .unwrap_or_else(|| root().join("runtime/target/release/dorobot-nano-runtime"))
}

pub fn configure(command: &mut Command) {
    if let Some(root) = resources() {
        let mut paths = vec![root.join("runtime/bin")];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocated_bundle_resolves_without_build_checkout() {
        let temp = std::env::temp_dir().join(format!("dorobot-bundle-test-{}", std::process::id()));
        let contents = temp.join("Moved Folder/DoRobot Studio.app/Contents");
        std::fs::create_dir_all(contents.join("Resources")).unwrap();
        std::fs::write(contents.join("Resources/release.json"), "{}").unwrap();
        assert_eq!(
            bundle_resources(&contents.join("MacOS/dorobot-flex")),
            Some(contents.join("Resources"))
        );
        assert_eq!(bundle_resources(&temp.join("debug/dorobot-flex")), None);
        std::fs::remove_dir_all(temp).unwrap();
    }
}
