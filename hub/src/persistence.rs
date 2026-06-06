use std::{fs, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use switcher_protocol::{DeviceIdentity, HealthStatus, RelayState};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedBridgeState {
    pub ble_address: Option<String>,
    pub relay_state: Option<RelayState>,
    pub identity: Option<DeviceIdentity>,
    pub health: Option<HealthStatus>,
}

impl PersistedBridgeState {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let state = PersistedBridgeState {
            ble_address: Some("AA:BB:CC:DD:EE:FF".into()),
            relay_state: Some(RelayState::On),
            identity: None,
            health: None,
        };

        let json = serde_json::to_vec(&state).unwrap();
        let decoded: PersistedBridgeState = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded.relay_state, Some(RelayState::On));
    }
}
