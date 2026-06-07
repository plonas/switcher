use switcher_protocol::{
    COMMAND_UUID, DeviceIdentity, HEALTH_UUID, HealthStatus, IDENTITY_UUID, ProtocolError,
    RelayCommand, RelayState, SERVICE_UUID, STATE_UUID,
};

use crate::FirmwareApp;

pub struct GattSnapshot {
    pub service_uuid: &'static str,
    pub state_uuid: &'static str,
    pub command_uuid: &'static str,
    pub health_uuid: &'static str,
    pub identity_uuid: &'static str,
    pub state_payload: [u8; 1],
    pub health_payload: [u8; HealthStatus::ENCODED_LEN],
    pub identity_payload: [u8; DeviceIdentity::ENCODED_LEN],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GattCharacteristic {
    State,
    Command,
    Health,
    Identity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GattValue {
    State([u8; 1]),
    Health([u8; HealthStatus::ENCODED_LEN]),
    Identity([u8; DeviceIdentity::ENCODED_LEN]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateNotification {
    pub state: RelayState,
    pub payload: [u8; 1],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GattAccessError {
    NotReadable(GattCharacteristic),
    NotWritable(GattCharacteristic),
    Protocol(ProtocolError),
}

impl core::fmt::Display for GattAccessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotReadable(characteristic) => {
                write!(f, "{characteristic:?} characteristic is not readable")
            }
            Self::NotWritable(characteristic) => {
                write!(f, "{characteristic:?} characteristic is not writable")
            }
            Self::Protocol(error) => error.fmt(f),
        }
    }
}

pub struct FirmwareGattServer {
    pending_state_notification: Option<StateNotification>,
}

impl FirmwareGattServer {
    pub const fn new() -> Self {
        Self {
            pending_state_notification: None,
        }
    }

    pub fn snapshot(&self, app: &FirmwareApp) -> GattSnapshot {
        GattSnapshot {
            service_uuid: SERVICE_UUID,
            state_uuid: STATE_UUID,
            command_uuid: COMMAND_UUID,
            health_uuid: HEALTH_UUID,
            identity_uuid: IDENTITY_UUID,
            state_payload: app.relay_state().encode(),
            health_payload: app.health_status().encode(),
            identity_payload: app.identity().encode(),
        }
    }

    pub fn read(
        &self,
        characteristic: GattCharacteristic,
        app: &FirmwareApp,
    ) -> Result<GattValue, GattAccessError> {
        match characteristic {
            GattCharacteristic::State => Ok(GattValue::State(app.relay_state().encode())),
            GattCharacteristic::Health => Ok(GattValue::Health(app.health_status().encode())),
            GattCharacteristic::Identity => Ok(GattValue::Identity(app.identity().encode())),
            GattCharacteristic::Command => Err(GattAccessError::NotReadable(characteristic)),
        }
    }

    pub fn write(
        &mut self,
        characteristic: GattCharacteristic,
        payload: &[u8],
        app: &mut FirmwareApp,
    ) -> Result<RelayState, GattAccessError> {
        if characteristic != GattCharacteristic::Command {
            return Err(GattAccessError::NotWritable(characteristic));
        }

        let command = RelayCommand::try_from(payload).map_err(GattAccessError::Protocol)?;
        let next_state = app.apply_command(command);
        self.pending_state_notification = Some(StateNotification {
            state: next_state,
            payload: next_state.encode(),
        });

        Ok(next_state)
    }

    pub fn take_notification(&mut self) -> Option<StateNotification> {
        self.pending_state_notification.take()
    }
}

pub struct GattModel;

impl GattModel {
    pub fn snapshot(
        state: RelayState,
        health: &HealthStatus,
        identity: &DeviceIdentity,
    ) -> GattSnapshot {
        GattSnapshot {
            service_uuid: SERVICE_UUID,
            state_uuid: STATE_UUID,
            command_uuid: COMMAND_UUID,
            health_uuid: HEALTH_UUID,
            identity_uuid: IDENTITY_UUID,
            state_payload: state.encode(),
            health_payload: health.encode(),
            identity_payload: identity.encode(),
        }
    }

    pub fn decode_command(payload: &[u8]) -> Result<RelayCommand, ProtocolError> {
        RelayCommand::try_from(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_write_updates_state_and_enqueues_notification() {
        let mut app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
        let mut server = FirmwareGattServer::new();

        let next = server
            .write(
                GattCharacteristic::Command,
                &RelayCommand::On.encode(),
                &mut app,
            )
            .unwrap();

        assert_eq!(next, RelayState::On);
        assert_eq!(
            server.take_notification(),
            Some(StateNotification {
                state: RelayState::On,
                payload: RelayState::On.encode(),
            })
        );
        assert_eq!(
            server.read(GattCharacteristic::State, &app).unwrap(),
            GattValue::State(RelayState::On.encode())
        );
    }

    #[test]
    fn read_access_matches_characteristic_capabilities() {
        let app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
        let server = FirmwareGattServer::new();

        assert!(matches!(
            server.read(GattCharacteristic::Command, &app),
            Err(GattAccessError::NotReadable(GattCharacteristic::Command))
        ));
        assert!(matches!(
            server.read(GattCharacteristic::Identity, &app),
            Ok(GattValue::Identity(_))
        ));
    }

    #[test]
    fn write_rejects_non_command_characteristics() {
        let mut app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
        let mut server = FirmwareGattServer::new();

        assert!(matches!(
            server.write(
                GattCharacteristic::State,
                &RelayCommand::On.encode(),
                &mut app
            ),
            Err(GattAccessError::NotWritable(GattCharacteristic::State))
        ));
    }
}
