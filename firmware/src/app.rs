use switcher_protocol::{DeviceIdentity, HealthCode, HealthStatus, RelayCommand, RelayState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareStatus {
    pub relay_state: RelayState,
    pub boot_count: u32,
    pub uptime_seconds: u32,
    pub last_error: HealthCode,
}

pub struct FirmwareApp {
    relay_state: RelayState,
    boot_count: u32,
    uptime_seconds: u32,
    last_error: HealthCode,
    identity: DeviceIdentity,
}

impl FirmwareApp {
    pub const fn new(device_id: [u8; 8], firmware_version: [u8; 3]) -> Self {
        Self {
            relay_state: RelayState::Off,
            boot_count: 1,
            uptime_seconds: 0,
            last_error: HealthCode::Ok,
            identity: DeviceIdentity::new(device_id, firmware_version),
        }
    }

    pub fn apply_command(&mut self, command: RelayCommand) -> RelayState {
        self.relay_state = match command {
            RelayCommand::Off => RelayState::Off,
            RelayCommand::On => RelayState::On,
            RelayCommand::Toggle => self.relay_state.toggle(),
        };

        self.relay_state
    }

    pub fn tick(&mut self, elapsed_seconds: u32) {
        self.uptime_seconds = self.uptime_seconds.saturating_add(elapsed_seconds);
    }

    pub fn set_last_error(&mut self, error: HealthCode) {
        self.last_error = error;
    }

    pub const fn relay_state(&self) -> RelayState {
        self.relay_state
    }

    pub fn health_status(&self) -> HealthStatus {
        HealthStatus::new(self.boot_count, self.uptime_seconds, self.last_error)
    }

    pub const fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    pub const fn snapshot(&self) -> FirmwareStatus {
        FirmwareStatus {
            relay_state: self.relay_state,
            boot_count: self.boot_count,
            uptime_seconds: self.uptime_seconds,
            last_error: self.last_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_off() {
        let app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
        assert_eq!(app.relay_state(), RelayState::Off);
    }

    #[test]
    fn applies_commands() {
        let mut app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
        assert_eq!(app.apply_command(RelayCommand::On), RelayState::On);
        assert_eq!(app.apply_command(RelayCommand::Toggle), RelayState::Off);
        assert_eq!(app.apply_command(RelayCommand::Off), RelayState::Off);
    }
}
