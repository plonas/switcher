use std::{io::Write, path::PathBuf, sync::Arc};

use anyhow::Result;
use env_logger::Env;
use hub::{
    BtleplugClient, BtleplugManager, DongleFleet, FleetSlot, HubConfig, MatterBridgeNode,
    MatterNodeConfig, device_id::parse_device_id, persistence::PersistedHubState,
};
use log::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("hub=info,btleplug=warn"))
        .format(|buf, record| writeln!(buf, "{}", record.args()))
        .init();

    let config_path = PathBuf::from("hub-config.json");
    let state_path = PathBuf::from("hub-state.json");

    let config = HubConfig::load(&config_path)?;
    let persisted = PersistedHubState::load(&state_path)?;
    info!("loaded {} registered dongles", config.dongles.len());

    let shared = BtleplugManager::new().await?;
    let mut slots = Vec::with_capacity(config.dongles.len());

    for registration in config.dongles {
        let device_id = parse_device_id(&registration.device_id)?;
        let state = persisted.state_for(&registration.device_id);
        let client = BtleplugClient::new(
            shared.clone(),
            Some(device_id),
            registration.preferred_ble_address.clone(),
            state.last_seen_ble_address.clone(),
        );
        slots.push(FleetSlot::new(registration, client, state));
    }

    let fleet = Arc::new(DongleFleet::new(slots));
    fleet.start_background_tasks().await;
    fleet.spawn_persistence_task(state_path);

    let matter = MatterBridgeNode::new(fleet.clone(), MatterNodeConfig::default());
    if let Err(error) = matter.run().await {
        warn!("Matter runtime not started: {error}");
        core::future::pending::<()>().await;
    }

    Ok(())
}
