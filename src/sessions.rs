use std::collections::HashMap;

use futures_util::StreamExt;
use logind_zbus::manager::ManagerProxy;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use zbus::{Connection, Proxy};

use crate::apparmor;
use crate::linux_device_policy;
use guardian_agent::{ClientMessage, build_alert_message};

#[derive(Clone, Debug, Default)]
struct SessionSnapshot {
    username: Option<String>,
    session_class: Option<String>,
    session_state: Option<String>,
}

fn is_user_session_class(session_class: Option<&str>) -> bool {
    session_class.is_some_and(|class_name| class_name.starts_with("user"))
}

async fn resolve_session_snapshot(
    connection: &Connection,
    object_path: &str,
) -> Option<SessionSnapshot> {
    let proxy = match Proxy::new(
        connection,
        "org.freedesktop.login1",
        object_path,
        "org.freedesktop.login1.Session",
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("Failed to create logind session proxy for {object_path}: {error}");
            return None;
        }
    };

    let username = proxy.get_property::<String>("Name").await.ok();
    let session_class = proxy.get_property::<String>("Class").await.ok();
    let session_state = proxy.get_property::<String>("State").await.ok();

    Some(SessionSnapshot {
        username,
        session_class,
        session_state,
    })
}

async fn run_session_listener(
    tx: mpsc::UnboundedSender<ClientMessage>,
    mut shutdown: watch::Receiver<bool>,
) {
    let connection = match Connection::system().await {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("Failed to connect to the system bus for session alerts: {error}");
            return;
        }
    };

    let proxy = match ManagerProxy::new(&connection).await {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("Failed to create logind manager proxy for session alerts: {error}");
            return;
        }
    };

    let mut session_new_stream = match proxy.receive_session_new().await {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("Failed to subscribe to SessionNew events: {error}");
            return;
        }
    };

    let mut session_removed_stream = match proxy.receive_session_removed().await {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("Failed to subscribe to SessionRemoved events: {error}");
            return;
        }
    };

    let mut session_cache: HashMap<String, SessionSnapshot> = HashMap::new();

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                break;
            }
            signal = session_new_stream.next() => {
                let Some(signal) = signal else {
                    eprintln!("SessionNew stream ended unexpectedly");
                    break;
                };

                match signal.args() {
                    Ok(args) => {
                        let session_id = args.session_id.to_string();
                        let object_path = args.object_path.to_string();
                        if let Some(snapshot) = resolve_session_snapshot(&connection, &object_path).await {
                            if is_user_session_class(snapshot.session_class.as_deref()) {
                                if let Some(ref uname) = snapshot.username {
                                    if let Err(error) = apparmor::load_profiles_for_user(uname).await {
                                        eprintln!("Failed to load AppArmor profiles for {uname}: {error}");
                                    }
                                }
                                if let Err(message) =
                                    linux_device_policy::refresh_active_session_from_logind(&connection).await
                                {
                                    eprintln!(
                                        "Failed to reconcile linux device policy after session start: {message}"
                                    );
                                }
                                let details = serde_json::json!({
                                    "session_id": session_id,
                                    "session_class": snapshot.session_class.clone(),
                                    "session_state": snapshot.session_state.clone(),
                                });
                                session_cache.insert(session_id.clone(), snapshot.clone());
                                if tx.send(build_alert_message("user_signed_in", snapshot.username.clone(), details)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to parse SessionNew signal: {error}");
                    }
                }
            }
            signal = session_removed_stream.next() => {
                let Some(signal) = signal else {
                    eprintln!("SessionRemoved stream ended unexpectedly");
                    break;
                };

                match signal.args() {
                    Ok(args) => {
                        let session_id = args.session_id.to_string();
                        let snapshot = session_cache.remove(&session_id).unwrap_or_default();
                        if let Some(ref uname) = snapshot.username {
                            if let Err(error) = apparmor::unload_profiles_for_user(uname).await {
                                eprintln!("Failed to unload AppArmor profiles for {uname}: {error}");
                            }
                        }
                        if let Err(message) =
                            linux_device_policy::refresh_active_session_from_logind(&connection).await
                        {
                            eprintln!(
                                "Failed to reconcile linux device policy after session end: {message}"
                            );
                        }
                        let details = serde_json::json!({
                            "session_id": session_id,
                            "session_class": snapshot.session_class.clone(),
                            "session_state": snapshot.session_state.clone(),
                        });
                        if tx.send(build_alert_message("user_signed_out", snapshot.username.clone(), details)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to parse SessionRemoved signal: {error}");
                    }
                }
            }
        }
    }
}

async fn run_sleep_listener(
    tx: mpsc::UnboundedSender<ClientMessage>,
    mut shutdown: watch::Receiver<bool>,
    resume_tx: Option<mpsc::UnboundedSender<()>>,
) {
    let connection = match Connection::system().await {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("Failed to connect to the system bus for sleep alerts: {error}");
            return;
        }
    };

    let proxy = match ManagerProxy::new(&connection).await {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("Failed to create logind manager proxy for sleep alerts: {error}");
            return;
        }
    };

    let mut sleep_stream = match proxy.receive_prepare_for_sleep().await {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("Failed to subscribe to PrepareForSleep events: {error}");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                break;
            }
            signal = sleep_stream.next() => {
                let Some(signal) = signal else {
                    eprintln!("PrepareForSleep stream ended unexpectedly");
                    break;
                };

                match signal.args() {
                    Ok(args) => {
                        let (event_type, phase) = if args.start {
                            ("system_sleep", "prepare")
                        } else {
                            ("system_resume", "resume")
                        };
                        let details = serde_json::json!({
                            "phase": phase,
                            "signal": "PrepareForSleep",
                        });
                        if tx.send(build_alert_message(event_type, None, details)).is_err() {
                            break;
                        }
                        if !args.start {
                            if let Some(resume) = resume_tx.as_ref() {
                                let _ = resume.send(());
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to parse PrepareForSleep signal: {error}");
                    }
                }
            }
        }
    }
}

async fn run_shutdown_listener(
    tx: mpsc::UnboundedSender<ClientMessage>,
    mut shutdown: watch::Receiver<bool>,
) {
    let connection = match Connection::system().await {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("Failed to connect to the system bus for shutdown alerts: {error}");
            return;
        }
    };

    let proxy = match ManagerProxy::new(&connection).await {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("Failed to create logind manager proxy for shutdown alerts: {error}");
            return;
        }
    };

    let mut shutdown_stream = match proxy.receive_prepare_for_shutdown().await {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("Failed to subscribe to PrepareForShutdown events: {error}");
            return;
        }
    };

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                break;
            }
            signal = shutdown_stream.next() => {
                let Some(signal) = signal else {
                    eprintln!("PrepareForShutdown stream ended unexpectedly");
                    break;
                };

                match signal.args() {
                    Ok(args) => {
                        if !args.start {
                            continue;
                        }
                        let details = serde_json::json!({
                            "phase": "prepare",
                            "signal": "PrepareForShutdown",
                        });
                        if tx.send(build_alert_message("system_restart", None, details)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to parse PrepareForShutdown signal: {error}");
                    }
                }
            }
        }
    }
}

pub fn spawn_logind_listeners(
    tx: mpsc::UnboundedSender<ClientMessage>,
    shutdown_rx: watch::Receiver<bool>,
    resume_tx: Option<mpsc::UnboundedSender<()>>,
) -> Vec<JoinHandle<()>> {
    vec![
        tokio::spawn(run_session_listener(tx.clone(), shutdown_rx.clone())),
        tokio::spawn(run_sleep_listener(
            tx.clone(),
            shutdown_rx.clone(),
            resume_tx,
        )),
        tokio::spawn(run_shutdown_listener(tx, shutdown_rx)),
    ]
}

#[cfg(test)]
mod tests {
    use super::is_user_session_class;

    #[test]
    fn user_session_class_filter_matches_systemd_user_sessions() {
        assert!(is_user_session_class(Some("user")));
        assert!(is_user_session_class(Some("user-light")));
        assert!(!is_user_session_class(Some("greeter")));
        assert!(!is_user_session_class(None));
    }
}
