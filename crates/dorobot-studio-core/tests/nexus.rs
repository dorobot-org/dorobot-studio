use dorobot_studio_core::api::nexus::{NexusIntent, NexusKind, NexusPreview, NexusState};

#[test]
fn recording_waits_for_reset_and_command_ack() {
    let mut state = NexusState::default();
    state.busy = true;
    state.kind = Some(NexusKind::Capture);
    state.snapshot.phase = "reset_required".into();
    assert!(state.allows(&NexusIntent::Reset));
    assert!(!state.allows(&NexusIntent::Record));
    state.snapshot.phase = "ready".into();
    assert!(state.allows(&NexusIntent::Record));
    state.command_pending = true;
    assert!(!state.allows(&NexusIntent::Record));
    assert!(state.allows(&NexusIntent::Stop));
    state.command_pending = false;
    state.snapshot.phase = "recording".into();
    assert!(state.allows(&NexusIntent::Finish));
    assert!(!state.allows(&NexusIntent::Save));
    state.kind = Some(NexusKind::Train);
    assert!(!state.allows(&NexusIntent::Finish));
}

#[test]
fn preview_requires_complete_rgb_and_new_sequence() {
    let mut packet = 45u64.to_be_bytes().to_vec();
    packet.extend(640u16.to_be_bytes());
    packet.extend(480u16.to_be_bytes());
    packet.resize(12 + 640 * 480 * 3, 25);
    assert_eq!(
        NexusPreview::decode(&packet, Some(44)).unwrap().rgb.len(),
        921600
    );
    assert!(NexusPreview::decode(&packet, Some(45)).is_none());
    assert!(NexusPreview::decode(&packet[..packet.len() - 1], None).is_none());
}

#[cfg(unix)]
#[test]
fn backend_supervises_local_worker_and_preserves_accepted_artifacts() {
    use dorobot_studio_core::api::{files::FileBackend, nexus::NexusSettings, Backend, Intent};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    // A protocol fixture substitutes for the Python training process. The
    // backend still creates the job, launches/reaps an OS process, loads its
    // status and sends a stop command through its normal control path.
    let root = std::env::temp_dir().join(format!(
        "nexus-backend-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    // The worker reports its physical working directory, including on macOS.
    let root = root.canonicalize().unwrap();
    let python = root.join("test-interpreter");
    fs::write(&python, r#"#!/bin/sh
set -eu
mkdir -p "$PWD/model" "$PWD/dataset/meta"
echo '{}' > "$PWD/dataset/meta/info.json"
printf '{"version":1,"phase":"training","message":"running","checkpoint":"%s/model","dataset":"%s/dataset","steps":2,"loss":0.5}' "$PWD" "$PWD" > status.tmp
mv status.tmp status.json
echo 'observed training step 2'
while ! test -f command.json; do sleep 0.02; done
cat command.json > observed.json
exit 0
"#).unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o700)).unwrap();
    let mut backend = FileBackend::new(&root);
    let mut settings = NexusSettings {
        python: python.display().to_string(),
        dataset: root.display().to_string(),
        output_root: root.display().to_string(),
        ..Default::default()
    };
    backend.dispatch(Intent::Nexus(NexusIntent::Configure(settings.clone())));
    backend.dispatch(Intent::Nexus(NexusIntent::Train));
    let wait = |b: &mut FileBackend, predicate: fn(&NexusState) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !predicate(b.nexus().unwrap()) && Instant::now() < deadline {
            b.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(predicate(b.nexus().unwrap()), "{:?}", b.nexus().unwrap());
    };
    wait(&mut backend, |s| s.snapshot.phase == "training");
    assert_eq!(backend.nexus().unwrap().loss_history, vec![(2, 0.5)]);
    let dir = backend.nexus().unwrap().job_dir.clone().unwrap();
    backend.dispatch(Intent::Nexus(NexusIntent::Stop));
    wait(&mut backend, |s| !s.busy);
    let state = backend.nexus().unwrap();
    assert_eq!(state.snapshot.phase, "stopped");
    assert!(state
        .recent_log
        .iter()
        .any(|line| line.contains("observed training")));
    let observed: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("observed.json")).unwrap()).unwrap();
    assert_eq!(observed["action"], "stop");
    assert_eq!(observed["id"], 1);
    assert!(!state.command_pending);
    assert!(
        state.snapshot.checkpoint.is_none(),
        "An interrupted trainer cannot publish its candidate"
    );
    fs::write(&python, r#"#!/bin/sh
mkdir model
echo '{}' > model/config.json
touch model/model.safetensors
printf '{"version":1,"phase":"completed","message":"trained","checkpoint":"%s/model"}' "$PWD" > status.json
"#).unwrap();
    backend.dispatch(Intent::Nexus(NexusIntent::Train));
    wait(&mut backend, |s| !s.busy);
    let trained = backend
        .nexus()
        .unwrap()
        .job_dir
        .clone()
        .unwrap()
        .join("model");
    assert_eq!(
        backend.nexus().unwrap().snapshot.checkpoint.as_ref(),
        Some(&trained)
    );
    settings.python = "/bin/false".into();
    backend.dispatch(Intent::Nexus(NexusIntent::Configure(settings)));
    backend.dispatch(Intent::Nexus(NexusIntent::Train));
    wait(&mut backend, |s| !s.busy);
    let state = backend.nexus().unwrap();
    assert_eq!(state.snapshot.phase, "failed");
    assert_eq!(state.snapshot.checkpoint.as_ref(), Some(&trained));
    assert_eq!(state.snapshot.dataset.as_ref(), Some(&dir.join("dataset")));
    fs::write(
        &python,
        r#"#!/bin/sh
echo '{"version":1,"phase":"training","message":"running"}' > status.json
while ! test -f command.json; do sleep 0.02; done
cat command.json > shutdown.json
"#,
    )
    .unwrap();
    let mut restart = state.settings.clone();
    restart.python = python.display().to_string();
    backend.dispatch(Intent::Nexus(NexusIntent::Configure(restart)));
    backend.dispatch(Intent::Nexus(NexusIntent::Train));
    wait(&mut backend, |s| s.snapshot.phase == "training");
    let active = backend.nexus().unwrap().job_dir.clone().unwrap();
    drop(backend);
    let shutdown: serde_json::Value =
        serde_json::from_slice(&fs::read(active.join("shutdown.json")).unwrap()).unwrap();
    assert_eq!(shutdown["action"], "stop");
    fs::remove_dir_all(root).unwrap();
}
