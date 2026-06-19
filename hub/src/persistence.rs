use std::{collections::BTreeMap, fs, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use switcher_protocol::{DeviceIdentity, HealthStatus, RelayState};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PersistedHubState {
    #[serde(default)]
    pub dongles: BTreeMap<String, PersistedDongleState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PersistedDongleState {
    pub last_seen_ble_address: Option<String>,
    pub last_relay_state: Option<RelayState>,
    pub identity: Option<DeviceIdentity>,
    pub health: Option<HealthStatus>,
}

impl PersistedHubState {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn state_for(&self, device_id: &str) -> PersistedDongleState {
        self.dongles.get(device_id).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_multiple_dongles() {
        let mut state = PersistedHubState::default();
        state.dongles.insert(
            "0011223344556677".into(),
            PersistedDongleState {
                last_seen_ble_address: Some("AA:BB:CC:DD:EE:FF".into()),
                last_relay_state: Some(RelayState::On),
                identity: None,
                health: None,
            },
        );
        state.dongles.insert(
            "8899aabbccddeeff".into(),
            PersistedDongleState {
                last_seen_ble_address: None,
                last_relay_state: Some(RelayState::Off),
                identity: None,
                health: None,
            },
        );

        let json = serde_json::to_vec(&state).unwrap();
        let decoded: PersistedHubState = serde_json::from_slice(&json).unwrap();

        assert_eq!(decoded, state);
    }
}
