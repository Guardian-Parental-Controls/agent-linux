//! Linux enforcement crate for the Guardian agent.

pub mod apparmor;
pub mod approval_deduper;
pub mod approval_policy;
pub mod audit_monitor;
pub mod bios_management;
pub mod clock_integrity_monitor;
pub mod commands;
pub mod domain_notify;
pub mod domain_policy;
pub mod extension_policy;
pub mod firewall;
pub mod installed_apps;
pub mod inventory;
pub mod ipc;
pub mod linux_device_policy;
pub mod local_dns;
pub mod netlink;
pub mod overlay;
pub mod policy_sync;
pub mod reporting_filter;
pub mod runtime;
pub mod schedule;
pub mod screenshot;
pub mod screenshots;
pub mod sessions;
pub mod terminal_monitor;
pub mod timekpr_dbus;
pub mod updater;
pub mod users;

pub use guardian_agent::build_alert_message;
pub use guardian_agent::i18n;
pub use guardian_agent::{ActiveClientTx, ClientMessage, LinuxUser};

#[cfg(target_os = "linux")]
pub async fn run_agent() {
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    println!("Starting Guardian Client Agent...");
    if let Err(message) = domain_policy::initialize_runtime().await {
        eprintln!("Failed to restore persisted domain policy: {message}");
    }
    if let Err(message) = apparmor::initialize_runtime().await {
        eprintln!("Failed to restore persisted AppArmor policy: {message}");
    }
    if let Err(message) = linux_device_policy::initialize_runtime().await {
        eprintln!("Failed to restore persisted Linux device policy: {message}");
    }

    let (alert_tx, mut alert_rx) = mpsc::unbounded_channel::<netlink::AppAlert>();
    let users_map = users::get_system_users_map();
    println!("Found regular system users: {users_map:?}");
    reporting_filter::refresh_user_indexes(users_map.values().map(String::as_str));

    let netlink_config = netlink::MonitorConfig {
        monitored_uids: users_map.clone(),
    };
    netlink::register_alert_sender(alert_tx.clone());
    tokio::spawn(netlink::run_process_monitor(
        netlink_config,
        alert_tx.clone(),
    ));
    tokio::spawn(audit_monitor::run_audit_monitor(
        users_map.clone(),
        alert_tx.clone(),
    ));
    tokio::spawn(terminal_monitor::run_terminal_monitor(
        users_map.clone(),
        alert_tx,
    ));

    let (clock_resume_tx, clock_resume_rx) = mpsc::unbounded_channel();
    let clock_monitor = Arc::new(clock_integrity_monitor::ClockIntegrityMonitor::new(
        users_map,
    ));
    clock_integrity_monitor::spawn_periodic_monitor(clock_monitor.clone());
    clock_integrity_monitor::spawn_resume_hook(clock_monitor, clock_resume_rx);
    clock_integrity_monitor::spawn_logind_resume_listener(clock_resume_tx);

    let active_client_tx = Arc::new(Mutex::new(None::<mpsc::UnboundedSender<ClientMessage>>));
    let active_tx_clone = active_client_tx.clone();
    tokio::spawn(async move {
        while let Some(alert) = alert_rx.recv().await {
            let msg = guardian_agent::build_alert_message(
                &alert.event_type,
                Some(alert.linux_username),
                alert.payload,
            );
            let opt_tx = {
                let guard = active_tx_clone.lock().unwrap();
                guard.clone()
            };
            if let Some(tx) = opt_tx {
                let _ = tx.send(msg);
            }
        }
    });

    let ipc_tx = active_client_tx.clone();
    tokio::spawn(async move {
        if let Err(error) = ipc::run_ipc_server(ipc_tx).await {
            eprintln!("Fatal error running local IPC server: {error}");
        }
    });

    guardian_agent::run_reconnect_loop(runtime::new_runtime(), active_client_tx).await;
}

#[cfg(not(target_os = "linux"))]
pub async fn run_agent() {
    eprintln!("guardian-agent-linux can only run on Linux");
    std::process::exit(1);
}
