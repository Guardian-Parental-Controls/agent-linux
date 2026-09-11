use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::bios_management;
use crate::commands;
use crate::inventory;
use crate::policy_sync;
use crate::screenshots;
use crate::sessions;
use crate::updater;
use crate::users;
use guardian_agent::{
    AgentRuntime, ClientMessage, CommandOutcome, HardwareInfo, LinuxUser, UpdateOffer,
};

#[derive(Debug, Default)]
pub struct LinuxAgent;

impl AgentRuntime for LinuxAgent {
    fn list_users(&self) -> Vec<LinuxUser> {
        users::get_system_users_map()
            .into_iter()
            .map(|(uid, username)| LinuxUser { username, uid })
            .collect()
    }

    fn hardware(&self) -> HardwareInfo {
        let detect = bios_management::detect_hardware_oem();
        HardwareInfo {
            oem: detect.oem,
            model: detect.model,
        }
    }

    fn startup_details(&self, hostname: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "source": "agent_service",
            "hostname": hostname,
        })
    }

    async fn handle_command(
        &self,
        action: &str,
        username: &str,
        args: &serde_json::Value,
    ) -> CommandOutcome {
        commands::handle_command(action, username, args).await
    }

    async fn apply_update(&self, offer: UpdateOffer) -> Result<(), String> {
        updater::apply_update(offer).await
    }

    async fn after_authenticate(&self, _client_tx: &mpsc::UnboundedSender<ClientMessage>) {}

    fn spawn_authenticated_tasks(
        &self,
        client_tx: mpsc::UnboundedSender<ClientMessage>,
        inventory_tx: mpsc::UnboundedSender<String>,
        shutdown: watch::Receiver<bool>,
        policy_sync_rx: mpsc::UnboundedReceiver<()>,
        screenshot_trigger_tx: mpsc::UnboundedSender<Option<String>>,
        screenshot_trigger_rx: mpsc::UnboundedReceiver<Option<String>>,
    ) -> Vec<JoinHandle<()>> {
        screenshots::set_screenshot_trigger_tx(screenshot_trigger_tx.clone());
        let mut handles =
            sessions::spawn_logind_listeners(client_tx.clone(), shutdown.clone(), None);
        handles.push(policy_sync::spawn_policy_sync_scheduler(
            client_tx,
            shutdown.clone(),
            policy_sync_rx,
        ));
        let inventory_users = users::get_system_users_map();
        inventory::push_inventory_for_users(&inventory_tx, &inventory_users);
        handles.push(inventory::spawn_installed_apps_scheduler(
            inventory_tx.clone(),
            inventory_users,
            shutdown.clone(),
        ));
        let screenshot_policy = screenshots::get_screenshot_policy_handle().clone();
        handles.push(screenshots::spawn_screenshot_capture_worker(
            screenshot_trigger_rx,
            inventory_tx,
            shutdown.clone(),
        ));
        handles.push(screenshots::spawn_screenshot_scheduler(
            screenshot_trigger_tx,
            screenshot_policy,
            shutdown,
        ));
        handles
    }

    fn push_inventory(&self, inventory_tx: &mpsc::UnboundedSender<String>, username: &str) {
        inventory::push_inventory_for_user(inventory_tx, username);
    }

    fn after_disconnect(&self) {
        screenshots::clear_screenshot_handles();
    }
}

pub fn new_runtime() -> Arc<LinuxAgent> {
    Arc::new(LinuxAgent)
}
