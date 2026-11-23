//! Zenoh client that subscribes to `dviz/**`, counts what the bridge publishes for
//! `--seconds` and prints a JSON summary; with `--expect expect.json` it exits 1 on any
//! violation (a missing entity, a rate or point count over its limit, too many bytes/s).
//!
//! ```text
//! dviz_probe --connect tcp/127.0.0.1:7450 --seconds 60 [--expect expect.json]
//! ```
//!
//! The ten nav entities are checked against the expectations. `logs`, `nodes` and `graph`
//! are counted separately (`extra_topics`); they only add to the byte total.

use dviz_protocol::{nav_entities, parse_message};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zenoh::Wait;

/// One received sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrival {
    /// Entity: the topic without the `dviz/<robot_id>/` prefix.
    pub entity: String,
    /// Seconds since the probe started.
    pub t: f64,
    pub bytes: usize,
    /// Point count of a points message.
    pub points: Option<u32>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stat {
    pub count: usize,
    pub bytes: usize,
    pub max_points: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub seconds: f64,
    pub entities: BTreeMap<String, Stat>,
    pub extra: BTreeMap<String, Stat>,
    pub total_bytes: usize,
}

#[derive(Debug, Default, Deserialize)]
pub struct EntityExpect {
    #[serde(default)]
    pub required: bool,
    pub max_hz: Option<f64>,
    pub max_points: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Expect {
    pub max_total_bytes_per_s: Option<f64>,
    #[serde(default)]
    pub entities: BTreeMap<String, EntityExpect>,
}

/// Fold arrivals into per-entity statistics. `seconds` is the run length.
pub fn summarize(arrivals: &[Arrival], seconds: f64) -> Summary {
    let mut entities = BTreeMap::new();
    let mut extra = BTreeMap::new();
    let mut total_bytes = 0;
    for a in arrivals {
        total_bytes += a.bytes;
        let map = if nav_entities::ALL.contains(&a.entity.as_str()) { &mut entities } else { &mut extra };
        let s: &mut Stat = map.entry(a.entity.clone()).or_default();
        s.count += 1;
        s.bytes += a.bytes;
        s.max_points = s.max_points.max(a.points);
    }
    Summary { seconds, entities, extra, total_bytes }
}

fn hz(s: &Stat, seconds: f64) -> f64 {
    if seconds > 0.0 {
        s.count as f64 / seconds
    } else {
        0.0
    }
}

/// Every violation of `expect` in `summary`, one line each (empty: all good).
/// Rate rule: the entity may send at most `max_hz * seconds + 1` messages (the +1 is the
/// startup publish that some entities make before their periodic cadence).
pub fn evaluate(summary: &Summary, expect: &Expect) -> Vec<String> {
    let mut out = Vec::new();
    for (name, e) in &expect.entities {
        let Some(s) = summary.entities.get(name) else {
            if e.required {
                out.push(format!("{name}: never received"));
            }
            continue;
        };
        if let Some(max_hz) = e.max_hz {
            if s.count as f64 > max_hz * summary.seconds + 1.0 {
                out.push(format!(
                    "{name}: {:.3} Hz ({} messages in {:.1} s) exceeds {max_hz} Hz",
                    hz(s, summary.seconds),
                    s.count,
                    summary.seconds
                ));
            }
        }
        if let (Some(max), Some(seen)) = (e.max_points, s.max_points) {
            if seen > max {
                out.push(format!("{name}: {seen} points exceeds {max}"));
            }
        }
    }
    if let Some(max) = expect.max_total_bytes_per_s {
        let rate = if summary.seconds > 0.0 { summary.total_bytes as f64 / summary.seconds } else { 0.0 };
        if rate > max {
            out.push(format!("total: {rate:.0} bytes/s exceeds {max:.0} bytes/s"));
        }
    }
    out
}

pub fn summary_json(summary: &Summary) -> Value {
    let stat = |s: &Stat| {
        json!({
            "count": s.count,
            "hz": (hz(s, summary.seconds) * 1000.0).round() / 1000.0,
            "bytes": s.bytes,
            "max_points": s.max_points,
        })
    };
    let ents: BTreeMap<_, _> = summary.entities.iter().map(|(k, v)| (k.clone(), stat(v))).collect();
    let extra: BTreeMap<_, _> = summary.extra.iter().map(|(k, v)| (k.clone(), stat(v))).collect();
    let bps = if summary.seconds > 0.0 { summary.total_bytes as f64 / summary.seconds } else { 0.0 };
    json!({
        "seconds": summary.seconds,
        "entities": ents,
        "extra_topics": extra,
        "total_bytes": summary.total_bytes,
        "total_bytes_per_s": bps.round(),
    })
}

/// `dviz/<robot_id>/<entity>` -> `<entity>`.
fn entity_of(key: &str) -> Option<&str> {
    let mut it = key.splitn(3, '/');
    (it.next()? == "dviz").then_some(())?;
    it.next()?;
    it.next().filter(|e| !e.is_empty())
}

fn usage() -> ! {
    eprintln!("usage: dviz_probe --connect tcp/<host>:7450 --seconds <n> [--expect expect.json]");
    std::process::exit(2)
}

fn main() {
    let (mut connect, mut seconds, mut expect_path) = (None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = args.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "--connect" => connect = Some(v),
            "--seconds" => seconds = v.parse::<f64>().ok().filter(|s| *s > 0.0),
            "--expect" => expect_path = Some(v),
            _ => usage(),
        }
    }
    let (Some(connect), Some(seconds)) = (connect, seconds) else { usage() };
    let expect: Option<Expect> = expect_path.map(|p| {
        let text = std::fs::read_to_string(&p).unwrap_or_else(|e| die(&format!("{p}: {e}")));
        serde_json::from_str(&text).unwrap_or_else(|e| die(&format!("{p}: {e}")))
    });

    let mut config = zenoh::Config::default();
    for (key, value) in [
        ("mode", r#""client""#.to_owned()),
        ("connect/endpoints", serde_json::to_string(&[&connect]).unwrap()),
        ("listen/endpoints", "[]".to_owned()),
        ("scouting/multicast/enabled", "false".to_owned()),
    ] {
        config.insert_json5(key, &value).unwrap_or_else(|e| die(&format!("zenoh config {key}: {e}")));
    }
    let session = zenoh::open(config).wait().unwrap_or_else(|e| die(&format!("zenoh open: {e}")));
    let start = Instant::now();
    let arrivals: Arc<Mutex<Vec<Arrival>>> = Arc::default();
    let sink = arrivals.clone();
    let _sub = session
        .declare_subscriber("dviz/**")
        .callback(move |sample| {
            let key = sample.key_expr().as_str().to_owned();
            let Some(entity) = entity_of(&key) else { return };
            let payload = sample.payload().to_bytes();
            let points = parse_message(&payload).and_then(|(h, _)| {
                (h.msg_type == "points3d").then(|| {
                    h.count.or_else(|| {
                        h.data.get("positions").and_then(Value::as_array).map(|a| a.len() as u32)
                    })
                })?
            });
            let a = Arrival {
                entity: entity.to_owned(),
                t: start.elapsed().as_secs_f64(),
                bytes: payload.len(),
                points,
            };
            sink.lock().unwrap_or_else(|e| e.into_inner()).push(a);
        })
        .wait()
        .unwrap_or_else(|e| die(&format!("zenoh subscribe: {e}")));

    std::thread::sleep(Duration::from_secs_f64(seconds));
    let got = arrivals.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let summary = summarize(&got, seconds);
    println!("{}", serde_json::to_string_pretty(&summary_json(&summary)).unwrap());
    if let Some(expect) = expect {
        let violations = evaluate(&summary, &expect);
        for v in &violations {
            eprintln!("VIOLATION: {v}");
        }
        if !violations.is_empty() {
            std::process::exit(1);
        }
        eprintln!("dviz_probe: all expectations met");
    }
}

fn die(msg: &str) -> ! {
    eprintln!("dviz_probe: {msg}");
    std::process::exit(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(entity: &str, rate_hz: f64, seconds: f64, bytes: usize, points: Option<u32>) -> Vec<Arrival> {
        let n = (rate_hz * seconds) as usize;
        (0..n)
            .map(|i| Arrival { entity: entity.into(), t: i as f64 / rate_hz, bytes, points })
            .collect()
    }

    fn expect() -> Expect {
        serde_json::from_str(include_str!("../tests/replay/expect.json")).unwrap()
    }

    /// Every entity once at a legal rate, so only the case under test can fail.
    fn healthy(seconds: f64) -> Vec<Arrival> {
        let mut v = Vec::new();
        for (e, r, p) in [
            ("map/grid", 0.2, None),
            ("map/cloud", 0.1, Some(40000)),
            ("robot/pose", 20.0, None),
            ("robot/footprint", 20.0, None),
            ("nav/plan", 1.0, None),
            ("nav/trail", 2.0, None),
            ("nav/goal", 0.5, None),
            ("sensors/scan", 10.0, None),
            ("sensors/cloud", 5.0, Some(5000)),
            ("nav/status", 5.0, None),
        ] {
            v.extend(arr(e, r, seconds, 100, p));
        }
        v
    }

    #[test]
    fn probe_summary_healthy_run_passes() {
        let s = summarize(&healthy(60.0), 60.0);
        assert_eq!(evaluate(&s, &expect()), Vec::<String>::new());
        assert_eq!(s.entities["robot/pose"].count, 1200);
        assert_eq!(s.entities["sensors/cloud"].max_points, Some(5000));
    }

    #[test]
    fn probe_summary_flags_rate_violation() {
        let mut a = healthy(60.0);
        a.retain(|x| x.entity != "robot/pose");
        a.extend(arr("robot/pose", 40.0, 60.0, 100, None)); // cap is 20 Hz
        let v = evaluate(&summarize(&a, 60.0), &expect());
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].starts_with("robot/pose:") && v[0].contains("exceeds"), "{v:?}");
    }

    #[test]
    fn probe_summary_flags_missing_points_and_bytes() {
        let mut a = healthy(60.0);
        a.retain(|x| x.entity != "nav/plan");
        a.iter_mut().filter(|x| x.entity == "sensors/cloud").for_each(|x| x.points = Some(5001));
        a.push(Arrival { entity: "map/cloud".into(), t: 1.0, bytes: 100_000_000, points: Some(50_000) });
        let v = evaluate(&summarize(&a, 60.0), &expect());
        assert!(v.iter().any(|m| m == "nav/plan: never received"), "{v:?}");
        assert!(v.iter().any(|m| m.contains("sensors/cloud: 5001 points")), "{v:?}");
        assert!(v.iter().any(|m| m.starts_with("total:")), "{v:?}");
        assert_eq!(v.len(), 3, "{v:?}");
    }

    #[test]
    fn probe_counts_extra_topics_separately() {
        let mut a = healthy(10.0);
        a.push(Arrival { entity: "logs".into(), t: 0.1, bytes: 10, points: None });
        a.push(Arrival { entity: "graph".into(), t: 0.2, bytes: 20, points: None });
        let s = summarize(&a, 10.0);
        assert_eq!(s.extra.keys().collect::<Vec<_>>(), ["graph", "logs"]);
        assert!(!s.entities.contains_key("logs"));
        assert!(s.total_bytes >= 30);
        assert!(evaluate(&s, &expect()).iter().all(|m| !m.starts_with("logs") && !m.starts_with("graph")));
    }

    #[test]
    fn entity_of_strips_prefix() {
        assert_eq!(entity_of("dviz/tracer/nav/status"), Some("nav/status"));
        assert_eq!(entity_of("dviz/tracer/logs"), Some("logs"));
        assert_eq!(entity_of("other/tracer/x"), None);
        assert_eq!(entity_of("dviz/tracer"), None);
    }
}
