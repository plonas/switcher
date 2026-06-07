#![no_std]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "std")]
extern crate std;

pub const PROTOCOL_VERSION: u8 = 1;
pub const DEVICE_NAME_PREFIX: &str = "switcher-dongle";

pub const SERVICE_UUID: &str = "0f0e0d0c-0b0a-4900-8100-001122334455";
pub const STATE_UUID: &str = "0f0e0d0c-0b0a-4900-8101-001122334455";
pub const COMMAND_UUID: &str = "0f0e0d0c-0b0a-4900-8102-001122334455";
pub const HEALTH_UUID: &str = "0f0e0d0c-0b0a-4900-8103-001122334455";
pub const IDENTITY_UUID: &str = "0f0e0d0c-0b0a-4900-8104-001122334455";

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum RelayCommand {
    Off = 0,
    On = 1,
    Toggle = 2,
}

impl RelayCommand {
    pub const fn encode(self) -> [u8; 1] {
        [self as u8]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum RelayState {
    Off = 0,
    On = 1,
}

impl RelayState {
    pub const fn as_bool(self) -> bool {
        matches!(self, Self::On)
    }

    pub const fn from_bool(value: bool) -> Self {
        if value { Self::On } else { Self::Off }
    }

    pub const fn toggle(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Off,
        }
    }

    pub const fn encode(self) -> [u8; 1] {
        [self as u8]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum HealthCode {
    Ok = 0,
    BleDisconnected = 1,
    InternalError = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, defmt::Format)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HealthStatus {
    pub protocol_version: u8,
    pub boot_count: u32,
    pub uptime_seconds: u32,
    pub last_error: HealthCode,
}

impl HealthStatus {
    pub const ENCODED_LEN: usize = 10;

    pub const fn new(boot_count: u32, uptime_seconds: u32, last_error: HealthCode) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            boot_count,
            uptime_seconds,
            last_error,
        }
    }

    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0_u8; Self::ENCODED_LEN];
        buf[0] = self.protocol_version;
        buf[1..5].copy_from_slice(&self.boot_count.to_le_bytes());
        buf[5..9].copy_from_slice(&self.uptime_seconds.to_le_bytes());
        buf[9] = self.last_error as u8;
        buf
    }
}

#[derive(Debug, Clone, PartialEq, Eq, defmt::Format)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DeviceIdentity {
    pub protocol_version: u8,
    pub device_id: [u8; 8],
    pub firmware_version: [u8; 3],
}

impl DeviceIdentity {
    pub const ENCODED_LEN: usize = 12;

    pub const fn new(device_id: [u8; 8], firmware_version: [u8; 3]) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            device_id,
            firmware_version,
        }
    }

    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut buf = [0_u8; Self::ENCODED_LEN];
        buf[0] = self.protocol_version;
        buf[1..9].copy_from_slice(&self.device_id);
        buf[9..12].copy_from_slice(&self.firmware_version);
        buf
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum ProtocolError {
    WrongLength,
    InvalidCommand(u8),
    InvalidState(u8),
    InvalidHealth(u8),
}

impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongLength => f.write_str("unexpected payload length"),
            Self::InvalidCommand(value) => write!(f, "invalid command value {value}"),
            Self::InvalidState(value) => write!(f, "invalid state value {value}"),
            Self::InvalidHealth(value) => write!(f, "invalid health code value {value}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ProtocolError {}

impl TryFrom<&[u8]> for RelayCommand {
    type Error = ProtocolError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 1 {
            return Err(ProtocolError::WrongLength);
        }

        match value[0] {
            0 => Ok(Self::Off),
            1 => Ok(Self::On),
            2 => Ok(Self::Toggle),
            other => Err(ProtocolError::InvalidCommand(other)),
        }
    }
}

impl TryFrom<&[u8]> for RelayState {
    type Error = ProtocolError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 1 {
            return Err(ProtocolError::WrongLength);
        }

        match value[0] {
            0 => Ok(Self::Off),
            1 => Ok(Self::On),
            other => Err(ProtocolError::InvalidState(other)),
        }
    }
}

impl TryFrom<&[u8]> for HealthStatus {
    type Error = ProtocolError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != Self::ENCODED_LEN {
            return Err(ProtocolError::WrongLength);
        }

        let last_error = match value[9] {
            0 => HealthCode::Ok,
            1 => HealthCode::BleDisconnected,
            2 => HealthCode::InternalError,
            other => return Err(ProtocolError::InvalidHealth(other)),
        };

        Ok(Self {
            protocol_version: value[0],
            boot_count: u32::from_le_bytes(value[1..5].try_into().unwrap()),
            uptime_seconds: u32::from_le_bytes(value[5..9].try_into().unwrap()),
            last_error,
        })
    }
}

impl TryFrom<&[u8]> for DeviceIdentity {
    type Error = ProtocolError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != Self::ENCODED_LEN {
            return Err(ProtocolError::WrongLength);
        }

        let mut device_id = [0_u8; 8];
        device_id.copy_from_slice(&value[1..9]);

        let mut firmware_version = [0_u8; 3];
        firmware_version.copy_from_slice(&value[9..12]);

        Ok(Self {
            protocol_version: value[0],
            device_id,
            firmware_version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_roundtrip() {
        let command = RelayCommand::Toggle;
        assert_eq!(RelayCommand::try_from(command.encode().as_slice()), Ok(command));
    }

    #[test]
    fn state_toggle_roundtrip() {
        let state = RelayState::On;
        assert_eq!(RelayState::try_from(state.encode().as_slice()), Ok(state));
        assert_eq!(state.toggle(), RelayState::Off);
    }

    #[test]
    fn health_roundtrip() {
        let status = HealthStatus::new(7, 42, HealthCode::BleDisconnected);
        let encoded = status.encode();
        assert_eq!(HealthStatus::try_from(encoded.as_slice()), Ok(status));
    }
}
