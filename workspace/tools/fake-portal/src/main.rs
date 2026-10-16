//! A stand-in for the desktop's GlobalShortcuts portal, for tests on a
//! private session bus (`dbus-run-session`): it accepts every session and
//! binding, prints each binding as a JSON line on stdout, and sends
//! `Activated` for `press <shortcut id>` lines on stdin.
//!
//!   cargo run --release   (in workspace/tools/fake-portal)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use zbus::message::Header;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{connection, Connection};

const PATH: &str = "/org/freedesktop/portal/desktop";

/// Last bound session, for `press`.
type Last = Arc<Mutex<Option<OwnedObjectPath>>>;

struct Portal {
    last: Last,
}

fn sender_part(header: &Header<'_>) -> String {
    header
        .sender()
        .map(|s| s.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default()
}

fn token(options: &HashMap<String, OwnedValue>, key: &str) -> String {
    options
        .get(key)
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .unwrap_or_else(|| "t".into())
}

/// Answer a request on its own path, a moment after the method returns.
fn respond(
    connection: &Connection,
    request: OwnedObjectPath,
    results: HashMap<&'static str, OwnedValue>,
) {
    let connection = connection.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = connection
            .emit_signal(
                None::<&str>,
                request.as_str(),
                "org.freedesktop.portal.Request",
                "Response",
                &(0u32, results),
            )
            .await;
    });
}

#[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
impl Portal {
    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> OwnedObjectPath {
        let sender = sender_part(&header);
        let request = OwnedObjectPath::try_from(format!(
            "{PATH}/request/{sender}/{}",
            token(&options, "handle_token")
        ))
        .unwrap();
        let session = format!(
            "{PATH}/session/{sender}/{}",
            token(&options, "session_handle_token")
        );
        let handle = OwnedValue::try_from(Value::from(session)).unwrap();
        respond(
            connection,
            request.clone(),
            HashMap::from([("session_handle", handle)]),
        );
        request
    }

    async fn bind_shortcuts(
        &self,
        session_handle: ObjectPath<'_>,
        shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> OwnedObjectPath {
        let bound: Vec<serde_json::Value> = shortcuts
            .iter()
            .map(|(id, props)| {
                serde_json::json!({
                    "id": id,
                    "description": token(props, "description"),
                    "trigger": token(props, "preferred_trigger"),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(bound));
        *self.last.lock().unwrap() = Some(session_handle.into());
        let request = OwnedObjectPath::try_from(format!(
            "{PATH}/request/{}/{}",
            sender_part(&header),
            token(&options, "handle_token")
        ))
        .unwrap();
        respond(connection, request.clone(), HashMap::new());
        request
    }

    #[zbus(signal)]
    async fn activated(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        session_handle: ObjectPath<'_>,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<&str, OwnedValue>,
    ) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let last: Last = Arc::default();
    let connection = connection::Builder::session()?
        .name("org.freedesktop.portal.Desktop")?
        .serve_at(PATH, Portal { last: last.clone() })?
        .build()
        .await?;
    eprintln!("fake portal up");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Some(id) = line.strip_prefix("press ") else {
            continue;
        };
        let Some(session) = last.lock().unwrap().clone() else {
            eprintln!("nothing bound");
            continue;
        };
        let emitter = zbus::object_server::SignalEmitter::new(&connection, PATH)?;
        Portal::activated(&emitter, session.as_ref(), id.trim(), 0, HashMap::new()).await?;
    }
    Ok(())
}
