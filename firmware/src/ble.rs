use switcher_protocol::{
    DeviceIdentity, HealthStatus, RelayCommand, RelayState, COMMAND_UUID, HEALTH_UUID,
    IDENTITY_UUID, SERVICE_UUID, STATE_UUID,
};

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

    pub fn decode_command(payload: &[u8]) -> Result<RelayCommand, switcher_protocol::ProtocolError> {
        RelayCommand::try_from(payload)
    }
}
