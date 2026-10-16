//! Global shortcuts (AUT.12): the show's shortcut keys also work while
//! the Console is not in front (a game, OBS, a browser), through the
//! desktop portal `org.freedesktop.portal.GlobalShortcuts` (KDE Plasma,
//! GNOME 48+, Hyprland). The desktop asks the user once to allow them and
//! may change the keys; it reports a press as `Activated`.
//!
//! The binding is redone whenever the shortcuts change: the session is
//! closed and a new one bound.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::watch;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::Connection;

/// A shortcut to bind: lmix shortcut id, its label and Console key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bind {
    pub id: String,
    pub label: String,
    pub key: String,
}

/// Wait before trying again when the portal is missing or refused.
const RETRY: Duration = Duration::from_secs(30);

#[zbus::proxy(
    interface = "org.freedesktop.portal.GlobalShortcuts",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait GlobalShortcuts {
    fn create_session(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<OwnedObjectPath>;

    fn bind_shortcuts(
        &self,
        session_handle: &ObjectPath<'_>,
        shortcuts: &[(&str, HashMap<&str, Value<'_>>)],
        parent_window: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    #[zbus(signal)]
    fn activated(
        &self,
        session_handle: ObjectPath<'_>,
        shortcut_id: &str,
        timestamp: u64,
        options: HashMap<&str, OwnedValue>,
    ) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Request",
    default_service = "org.freedesktop.portal.Desktop"
)]
trait Request {
    #[zbus(signal)]
    fn response(&self, response: u32, results: HashMap<&str, OwnedValue>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Session",
    default_service = "org.freedesktop.portal.Desktop"
)]
trait Session {
    fn close(&self) -> zbus::Result<()>;
}

/// A Qt key sequence (`Ctrl+Shift+F1`, `Alt+A`, `Space`) as an XDG
/// shortcut trigger (`CTRL+SHIFT+F1`, `ALT+a`, `space`).
pub fn trigger(key: &str) -> String {
    key.split('+')
        .map(|part| match part {
            "Ctrl" => "CTRL".to_string(),
            "Alt" => "ALT".to_string(),
            "Shift" => "SHIFT".to_string(),
            "Meta" => "LOGO".to_string(),
            "Space" => "space".to_string(),
            single if single.chars().count() == 1 => single.to_lowercase(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Where the portal will post the answer to a request with `token`: known
/// before the call, so the answer cannot be missed.
fn request_path(unique_name: &str, token: &str) -> String {
    let sender = unique_name.trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
}

/// Keep `binds` bound until the sender goes away. `activated` gets the id
/// of a pressed shortcut, `status` a line for the Console ("" when off).
pub async fn run(
    mut binds: watch::Receiver<Vec<Bind>>,
    activated: impl Fn(String) + Send + 'static,
    status: impl Fn(String) + Send + 'static,
) {
    let mut serial = 0u32;
    loop {
        let wanted = binds.borrow_and_update().clone();
        if wanted.is_empty() {
            status(String::new());
            if binds.changed().await.is_err() {
                return;
            }
            continue;
        }
        serial += 1;
        match bind(&wanted, serial).await {
            Ok((connection, portal, session)) => {
                status(format!("on:{}", wanted.len()));
                let Ok(mut presses) = portal.receive_activated().await else {
                    continue;
                };
                let gone = loop {
                    tokio::select! {
                        press = presses.next() => {
                            let Some(press) = press else { break false };
                            if let Ok(args) = press.args() {
                                if args.session_handle.as_str() == session.as_str() {
                                    activated(args.shortcut_id.to_string());
                                }
                            }
                        }
                        changed = binds.changed() => break changed.is_err(),
                    }
                };
                close(&connection, &session).await;
                if gone {
                    return;
                }
            }
            Err(error) => {
                tracing::info!(%error, "global shortcuts unavailable");
                status(format!("off:{error}"));
                tokio::select! {
                    changed = binds.changed() => if changed.is_err() { return },
                    _ = tokio::time::sleep(RETRY) => {}
                }
            }
        }
    }
}

/// A new portal session with `binds`; its connection, proxy and handle.
async fn bind(
    binds: &[Bind],
    serial: u32,
) -> zbus::Result<(Connection, GlobalShortcutsProxy<'static>, OwnedObjectPath)> {
    let connection = Connection::session().await?;
    let portal = GlobalShortcutsProxy::new(&connection).await?;

    let token = format!("lmix_session_{serial}");
    let results = request(&connection, &token, async {
        let options = HashMap::from([
            ("handle_token", Value::from(token.as_str())),
            ("session_handle_token", Value::from(token.as_str())),
        ]);
        portal.create_session(options).await
    })
    .await?;
    let session = results
        .get("session_handle")
        .and_then(|v| {
            String::try_from(v.clone()).ok().or_else(|| {
                OwnedObjectPath::try_from(v.clone())
                    .ok()
                    .map(|p| p.to_string())
            })
        })
        .and_then(|s| OwnedObjectPath::try_from(s).ok())
        .ok_or_else(|| zbus::Error::Failure("portal gave no session".into()))?;

    let shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = binds
        .iter()
        .map(|b| {
            let properties = HashMap::from([
                ("description", Value::from(b.label.as_str())),
                ("preferred_trigger", Value::from(trigger(&b.key))),
            ]);
            (b.id.as_str(), properties)
        })
        .collect();
    let token = format!("lmix_bind_{serial}");
    request(&connection, &token, async {
        let options = HashMap::from([("handle_token", Value::from(token.as_str()))]);
        portal
            .bind_shortcuts(&session.as_ref(), &shortcuts, "", options)
            .await
    })
    .await?;
    Ok((connection, portal, session))
}

/// Run a portal call and wait for its `Response`.
async fn request(
    connection: &Connection,
    token: &str,
    call: impl std::future::Future<Output = zbus::Result<OwnedObjectPath>>,
) -> zbus::Result<HashMap<String, OwnedValue>> {
    let unique = connection
        .unique_name()
        .ok_or_else(|| zbus::Error::Failure("no bus name".into()))?
        .to_string();
    let answer = RequestProxy::builder(connection)
        .path(request_path(&unique, token))?
        .build()
        .await?;
    let mut responses = answer.receive_response().await?;
    call.await?;
    let response = responses
        .next()
        .await
        .ok_or_else(|| zbus::Error::Failure("portal went away".into()))?;
    let args = response.args()?;
    if args.response != 0 {
        return Err(zbus::Error::Failure(
            "the desktop refused global shortcuts".into(),
        ));
    }
    Ok(args
        .results
        .iter()
        .map(|(k, v)| (k.to_string(), v.try_clone().expect("plain value")))
        .collect())
}

async fn close(connection: &Connection, session: &OwnedObjectPath) {
    if let Ok(builder) = SessionProxy::builder(connection).path(session.clone()) {
        if let Ok(proxy) = builder.build().await {
            let _ = proxy.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_keys_become_xdg_triggers() {
        assert_eq!(trigger("Ctrl+Shift+F1"), "CTRL+SHIFT+F1");
        assert_eq!(trigger("Alt+A"), "ALT+a");
        assert_eq!(trigger("Meta+Space"), "LOGO+space");
        assert_eq!(trigger("F12"), "F12");
    }

    #[test]
    fn request_paths_follow_the_sender() {
        assert_eq!(
            request_path(":1.42", "lmix_bind_3"),
            "/org/freedesktop/portal/desktop/request/1_42/lmix_bind_3"
        );
    }
}
