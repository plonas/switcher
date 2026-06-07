use std::sync::Arc;

use anyhow::Result;
use log::{info, warn};
use switcher_protocol::{DeviceIdentity, HealthStatus, RelayCommand, RelayState};
use tokio::sync::{Mutex, broadcast, watch};

use crate::ble::{BleBridgeClient, BleNotification};

#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub connected: bool,
    pub relay_state: RelayState,
    pub health: Option<HealthStatus>,
    pub identity: Option<DeviceIdentity>,
}

impl Default for BridgeStatus {
    fn default() -> Self {
        Self {
            connected: false,
            relay_state: RelayState::Off,
            health: None,
            identity: None,
        }
    }
}

pub struct RelayBridge<C> {
    client: Arc<C>,
    status: Arc<Mutex<BridgeStatus>>,
    watch_tx: watch::Sender<BridgeStatus>,
}

impl<C> RelayBridge<C>
where
    C: BleBridgeClient + 'static,
{
    pub fn new(client: C) -> Self {
        Self::new_with_status(client, BridgeStatus::default())
    }

    pub fn new_with_status(client: C, status: BridgeStatus) -> Self {
        let (watch_tx, _) = watch::channel(status.clone());

        Self {
            client: Arc::new(client),
            status: Arc::new(Mutex::new(status)),
            watch_tx,
        }
    }

    pub async fn connect(&self) -> Result<()> {
        self.connect_and_sync().await
    }

    pub async fn start_notification_task(&self) {
        let mut rx = self.client.subscribe();
        let status = self.status.clone();
        let watch_tx = self.watch_tx.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(BleNotification { state }) => {
                        let mut guard = status.lock().await;
                        guard.connected = true;
                        guard.relay_state = state;
                        let _ = watch_tx.send(guard.clone());
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("missed {skipped} BLE notifications");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub async fn start_background_tasks(&self) {
        self.start_notification_task().await;

        let client = self.client.clone();
        let status = self.status.clone();
        let watch_tx = self.watch_tx.clone();

        tokio::spawn(async move {
            loop {
                let was_connected = status.lock().await.connected;

                if let Err(error) = client.connect().await {
                    if was_connected {
                        let mut guard = status.lock().await;
                        guard.connected = false;
                        let _ = watch_tx.send(guard.clone());
                    }
                    warn!("BLE bridge unavailable, retrying: {error}");
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    continue;
                }

                let relay_state = match client.current_state().await {
                    Ok(state) => state,
                    Err(error) => {
                        let mut guard = status.lock().await;
                        guard.connected = false;
                        let _ = watch_tx.send(guard.clone());
                        warn!("BLE bridge state sync failed, retrying: {error}");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                let health = client.health().await.ok();
                let identity = client.identity().await.ok();

                {
                    let mut guard = status.lock().await;
                    guard.connected = true;
                    guard.relay_state = relay_state;
                    guard.health = health;
                    guard.identity = identity;
                    let _ = watch_tx.send(guard.clone());
                }

                if !was_connected {
                    info!("BLE bridge connected");
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    }

    pub async fn set_state(&self, state: RelayState) -> Result<RelayState> {
        let command = match state {
            RelayState::Off => RelayCommand::Off,
            RelayState::On => RelayCommand::On,
        };

        self.issue(command).await
    }

    pub async fn toggle(&self) -> Result<RelayState> {
        self.issue(RelayCommand::Toggle).await
    }

    pub async fn issue(&self, command: RelayCommand) -> Result<RelayState> {
        let next = match self.client.send_command(command).await {
            Ok(next) => next,
            Err(error) => {
                self.update_status(|status| {
                    status.connected = false;
                })
                .await;
                return Err(error);
            }
        };
        self.update_status(|status| {
            status.connected = true;
            status.relay_state = next;
        })
        .await;
        Ok(next)
    }

    pub fn subscribe(&self) -> watch::Receiver<BridgeStatus> {
        self.watch_tx.subscribe()
    }

    pub async fn status(&self) -> BridgeStatus {
        self.status.lock().await.clone()
    }

    async fn update_status(&self, f: impl FnOnce(&mut BridgeStatus)) {
        let mut guard = self.status.lock().await;
        f(&mut guard);
        let _ = self.watch_tx.send(guard.clone());
    }

    async fn connect_and_sync(&self) -> Result<()> {
        self.client.connect().await?;
        let relay_state = self.client.current_state().await?;
        let health = self.client.health().await.ok();
        let identity = self.client.identity().await.ok();

        self.update_status(|status| {
            status.connected = true;
            status.relay_state = relay_state;
            status.health = health;
            status.identity = identity;
        })
        .await;

        info!("BLE bridge connected");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble::MockBleClient;

    #[tokio::test]
    async fn bridge_tracks_state() {
        let bridge = RelayBridge::new(MockBleClient::new());
        bridge.connect().await.unwrap();
        bridge.start_notification_task().await;

        assert_eq!(
            bridge.set_state(RelayState::On).await.unwrap(),
            RelayState::On
        );
        assert_eq!(bridge.toggle().await.unwrap(), RelayState::Off);
        assert_eq!(bridge.status().await.relay_state, RelayState::Off);
    }

    #[tokio::test]
    async fn bridge_marks_disconnected_after_command_failure() {
        let client = MockBleClient::new();
        let bridge = RelayBridge::new(client.clone());
        bridge.connect().await.unwrap();

        client.disconnect().await;

        let error = bridge.toggle().await.unwrap_err();
        assert!(error.to_string().contains("disconnected"));
        assert!(!bridge.status().await.connected);
    }
}
