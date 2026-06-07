use std::{io::Write, path::PathBuf, sync::Arc};

use anyhow::Result;
use env_logger::Env;
use log::{info, warn};
use switcher_hub::{
    BridgeStatus, BtleplugClient, MatterBridgeNode, MatterNodeConfig, RelayBridge,
    persistence::PersistedBridgeState,
};
use switcher_protocol::RelayState;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| writeln!(buf, "{}", record.args()))
        .init();

    let persistence_path = PathBuf::from("hub-state.json");
    let persisted = PersistedBridgeState::load(&persistence_path)?;
    info!("loaded persisted bridge state: {:?}", persisted.relay_state);

    let client = BtleplugClient::new().await?;
    let bridge = Arc::new(RelayBridge::new_with_status(
        client,
        BridgeStatus {
            connected: false,
            relay_state: persisted.relay_state.unwrap_or(RelayState::Off),
            health: persisted.health.clone(),
            identity: persisted.identity.clone(),
        },
    ));
    bridge.start_background_tasks().await;

    let matter = MatterBridgeNode::new(bridge.clone(), MatterNodeConfig::default());
    if let Err(error) = matter.run().await {
        warn!("Matter runtime not started: {error}");
    }

    let status = bridge.status().await;
    PersistedBridgeState {
        ble_address: None,
        relay_state: Some(status.relay_state),
        identity: status.identity,
        health: status.health,
    }
    .save(&persistence_path)?;

    Ok(())
}
