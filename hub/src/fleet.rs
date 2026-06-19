use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::RwLock;

use crate::{
    ble::BleBridgeClient,
    bridge::{BridgeStatus, RelayBridge},
    config::RegisteredDongle,
    persistence::{PersistedDongleState, PersistedHubState},
};

pub struct FleetSlot<C> {
    pub registration: RegisteredDongle,
    pub bridge: Arc<RelayBridge<C>>,
}

pub struct DongleFleet<C> {
    slots: Vec<FleetSlot<C>>,
}

impl<C> DongleFleet<C>
where
    C: BleBridgeClient + 'static,
{
    pub fn new(slots: Vec<FleetSlot<C>>) -> Self {
        Self { slots }
    }

    pub fn slots(&self) -> &[FleetSlot<C>] {
        &self.slots
    }

    pub async fn start_background_tasks(&self) {
        for slot in &self.slots {
            slot.bridge.start_background_tasks().await;
        }
    }

    pub async fn status_map(&self) -> HashMap<String, BridgeStatus> {
        let mut map = HashMap::with_capacity(self.slots.len());

        for slot in &self.slots {
            map.insert(slot.registration.device_id.clone(), slot.bridge.status().await);
        }

        map
    }

    pub async fn persisted_state(&self) -> PersistedHubState {
        let mut state = PersistedHubState::default();

        for slot in &self.slots {
            let status = slot.bridge.status().await;
            state.dongles.insert(
                slot.registration.device_id.clone(),
                PersistedDongleState {
                    last_seen_ble_address: status.ble_address,
                    last_relay_state: Some(status.relay_state),
                    identity: status.identity,
                    health: status.health,
                },
            );
        }

        state
    }

    pub fn spawn_persistence_task(self: &Arc<Self>, path: PathBuf) {
        let fleet = self.clone();
        let last_saved = Arc::new(RwLock::new(None::<PersistedHubState>));

        tokio::spawn(async move {
            loop {
                let next = fleet.persisted_state().await;
                let should_save = {
                    let guard = last_saved.read().await;
                    guard.as_ref() != Some(&next)
                };

                if should_save {
                    if next.save(&path).is_ok() {
                        *last_saved.write().await = Some(next);
                    }
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    pub fn enabled_slots(&self) -> impl Iterator<Item = &FleetSlot<C>> {
        self.slots.iter().filter(|slot| slot.registration.enabled)
    }
}

impl<C> FleetSlot<C>
where
    C: BleBridgeClient + 'static,
{
    pub fn new(
        registration: RegisteredDongle,
        client: C,
        persisted: PersistedDongleState,
    ) -> FleetSlot<C> {
        let bridge = Arc::new(RelayBridge::new_with_status(
            client,
            BridgeStatus {
                ble_address: persisted.last_seen_ble_address,
                connected: false,
                relay_state: persisted
                    .last_relay_state
                    .unwrap_or(switcher_protocol::RelayState::Off),
                health: persisted.health,
                identity: persisted.identity,
            },
        ));

        FleetSlot { registration, bridge }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble::MockBleClient;

    #[tokio::test]
    async fn fleet_tracks_multiple_mock_bridges_independently() {
        let slot_a = FleetSlot::new(
            RegisteredDongle {
                device_id: "0011223344556677".into(),
                endpoint_id: 2,
                enabled: true,
                preferred_ble_address: None,
            },
            MockBleClient::with_device(*b"dongle-a", Some("AA:BB:CC:DD:EE:01".into())),
            PersistedDongleState::default(),
        );
        let slot_b = FleetSlot::new(
            RegisteredDongle {
                device_id: "8899aabbccddeeff".into(),
                endpoint_id: 3,
                enabled: true,
                preferred_ble_address: None,
            },
            MockBleClient::with_device(*b"dongle-b", Some("AA:BB:CC:DD:EE:02".into())),
            PersistedDongleState::default(),
        );

        let fleet = DongleFleet::new(vec![slot_a, slot_b]);
        let slot_a = &fleet.slots()[0];
        let slot_b = &fleet.slots()[1];

        slot_a.bridge.connect().await.unwrap();
        slot_b.bridge.connect().await.unwrap();
        slot_a.bridge.start_notification_task().await;
        slot_b.bridge.start_notification_task().await;

        slot_a
            .bridge
            .set_state(switcher_protocol::RelayState::On)
            .await
            .unwrap();
        slot_b
            .bridge
            .set_state(switcher_protocol::RelayState::Off)
            .await
            .unwrap();

        assert_eq!(
            slot_a.bridge.status().await.relay_state,
            switcher_protocol::RelayState::On
        );
        assert_eq!(
            slot_b.bridge.status().await.relay_state,
            switcher_protocol::RelayState::Off
        );
    }
}
