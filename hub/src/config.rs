use std::{collections::HashSet, fs, path::Path};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::{ble::normalize_ble_address, device_id::parse_device_id};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HubConfig {
    #[serde(default)]
    pub dongles: Vec<RegisteredDongle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisteredDongle {
    pub device_id: String,
    pub endpoint_id: u16,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub preferred_ble_address: Option<String>,
}

impl HubConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let bytes = fs::read(path)?;
        let mut config: Self = serde_json::from_slice(&bytes)?;
        config.normalize()?;
        Ok(config)
    }

    pub fn enabled_dongles(&self) -> impl Iterator<Item = &RegisteredDongle> {
        self.dongles.iter().filter(|dongle| dongle.enabled)
    }

    fn normalize(&mut self) -> Result<()> {
        let mut device_ids = HashSet::new();
        let mut endpoint_ids = HashSet::new();

        for dongle in &mut self.dongles {
            let normalized = dongle.device_id.trim().to_ascii_lowercase();
            let parsed = parse_device_id(&normalized)?;
            dongle.device_id = crate::device_id::format_device_id(&parsed);
            dongle.preferred_ble_address = dongle
                .preferred_ble_address
                .as_deref()
                .and_then(normalize_ble_address);

            if !device_ids.insert(dongle.device_id.clone()) {
                bail!("duplicate device_id {} in hub-config.json", dongle.device_id);
            }

            if !endpoint_ids.insert(dongle.endpoint_id) {
                bail!("duplicate endpoint_id {} in hub-config.json", dongle.endpoint_id);
            }

            if dongle.endpoint_id <= 1 {
                bail!(
                    "endpoint_id {} is reserved; bridged dongles must use endpoint_id >= 2",
                    dongle.endpoint_id
                );
            }
        }

        self.dongles.sort_by_key(|dongle| dongle.endpoint_id);
        Ok(())
    }
}

const fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_multiple_dongles() {
        let json = serde_json::json!({
            "dongles": [
                {
                    "device_id": "0011223344556677",
                    "endpoint_id": 2,
                    "enabled": true,
                    "preferred_ble_address": "AA:BB:CC:DD:EE:FF"
                },
                {
                    "device_id": "8899aabbccddeeff",
                    "endpoint_id": 3,
                    "enabled": false
                }
            ]
        });

        let mut config: HubConfig = serde_json::from_value(json).unwrap();
        config.normalize().unwrap();

        assert_eq!(config.dongles.len(), 2);
        assert_eq!(config.dongles[0].device_id, "0011223344556677");
        assert_eq!(
            config.dongles[0].preferred_ble_address.as_deref(),
            Some("AA:BB:CC:DD:EE:FF")
        );
        assert!(!config.dongles[1].enabled);
    }
}
