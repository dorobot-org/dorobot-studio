//! Output side: the bridge's Zenoh session and one publisher per dviz entity,
//! drop-style congestion control.

use crate::config::BridgeConfig;
use crate::logs;
use dviz_protocol::nav_entities;
use zenoh::{pubsub::Publisher, qos::CongestionControl, Wait};

/// Where the worker sends encoded messages.
pub trait Publish: Send {
    /// Whether anyone subscribes to `entity` (Zenoh matching status).
    fn wants(&self, entity: &str) -> bool;
    /// Send without blocking on the reader; under congestion the message is dropped.
    fn put(&mut self, entity: &str, bytes: Vec<u8>);
}

struct ZenohPublish {
    // Declared before the session so the publishers are undeclared first on drop.
    publishers: Vec<(&'static str, Publisher<'static>)>,
    _session: zenoh::Session,
}

impl ZenohPublish {
    fn find(&self, entity: &str) -> Option<&Publisher<'static>> {
        self.publishers.iter().find(|(e, _)| *e == entity).map(|(_, p)| p)
    }
}

impl Publish for ZenohPublish {
    fn wants(&self, entity: &str) -> bool {
        self.find(entity)
            .and_then(|p| p.matching_status().wait().ok())
            .is_some_and(|s| s.matching())
    }

    /// With `CongestionControl::Drop`, Zenoh waits at most `wait_before_drop` (1 ms by
    /// default; 50 ms for a fragmented message such as `map/cloud`) for queue space.
    fn put(&mut self, entity: &str, bytes: Vec<u8>) {
        if let Some(p) = self.find(entity) {
            // Congestion drops are not errors; an error (e.g. a closed session) means
            // this message is not sent, and there is nothing to retry.
            let _ = p.put(bytes).wait();
        }
    }
}

/// The bridge's Zenoh session: peer mode, listening on `cfg.listen`, no outgoing
/// connections, multicast scouting off. The bridge owns it; the publishers and the
/// command queryable are declared on it.
pub fn open_session(cfg: &BridgeConfig) -> Result<zenoh::Session, String> {
    let listen = serde_json::to_string(&cfg.listen).map_err(|e| e.to_string())?;
    let mut config = zenoh::Config::default();
    for (key, value) in [
        ("mode", r#""peer""#),
        ("listen/endpoints", listen.as_str()),
        ("connect/endpoints", "[]"),
        ("scouting/multicast/enabled", "false"),
    ] {
        config.insert_json5(key, value).map_err(|e| format!("zenoh config {key}: {e}"))?;
    }
    zenoh::open(config)
        .wait()
        .map_err(|e| format!("zenoh open (listen {}): {e}", cfg.listen.join(", ")))
}

/// One publisher per entity in `nav_entities::ALL` and `logs::ENTITIES` on
/// `dviz/<robot_id>/<entity>`, declared on `session`. It holds a clone of the session:
/// dropping it (the worker ending or dying) closes the session only if nobody else
/// holds one.
pub fn zenoh_publisher_on(session: &zenoh::Session, cfg: &BridgeConfig) -> Result<Box<dyn Publish>, String> {
    let entities = nav_entities::ALL.iter().chain(logs::ENTITIES.iter()).copied();
    let mut publishers = Vec::with_capacity(nav_entities::ALL.len() + logs::ENTITIES.len());
    for entity in entities {
        let key = format!("dviz/{}/{entity}", cfg.robot_id);
        let publisher = session
            .declare_publisher(key.clone())
            .congestion_control(CongestionControl::Drop)
            .wait()
            .map_err(|e| format!("zenoh publisher {key}: {e}"))?;
        publishers.push((entity, publisher));
    }
    Ok(Box::new(ZenohPublish { publishers, _session: session.clone() }))
}

/// [`open_session`] and [`zenoh_publisher_on`] in one: the publisher is the session's
/// only holder, so dropping it closes the session.
pub fn zenoh_publisher(cfg: &BridgeConfig) -> Result<Box<dyn Publish>, String> {
    let session = open_session(cfg)?;
    zenoh_publisher_on(&session, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_listen_endpoint_and_robot_id_are_errors() {
        let cfg = BridgeConfig { listen: vec!["nonsense".into()], ..BridgeConfig::default() };
        assert!(zenoh_publisher(&cfg).is_err());
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let cfg = BridgeConfig {
            robot_id: "a//b".into(),
            listen: vec![format!("tcp/127.0.0.1:{port}")],
            ..BridgeConfig::default()
        };
        let err = zenoh_publisher(&cfg).err().unwrap();
        assert!(err.contains("dviz/a//b"), "{err}");
    }

    /// The bridge owns the session: dropping the publisher (the worker ending or dying)
    /// leaves it open for the command queryable.
    #[test]
    fn publisher_shares_the_bridge_session() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let cfg = BridgeConfig { listen: vec![format!("tcp/127.0.0.1:{port}")], ..BridgeConfig::default() };
        let session = open_session(&cfg).unwrap();
        let publisher = zenoh_publisher_on(&session, &cfg).unwrap();
        drop(publisher);
        assert!(!session.is_closed());
        session.close().wait().unwrap();
    }

    #[test]
    fn port_in_use_is_an_error() {
        let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let cfg = BridgeConfig { listen: vec![format!("tcp/127.0.0.1:{port}")], ..BridgeConfig::default() };
        let err = zenoh_publisher(&cfg).err().unwrap();
        assert!(err.contains(&port.to_string()), "{err}");
    }
}
