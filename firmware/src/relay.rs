use embedded_hal::digital::v2::OutputPin;
use nrf52840_hal::gpio::{Output, Pin, PushPull};
use switcher_protocol::RelayState;

pub struct RelayOutput {
    pin: Pin<Output<PushPull>>,
    state: RelayState,
}

impl RelayOutput {
    pub fn new(pin: Pin<Output<PushPull>>) -> Self {
        Self {
            pin,
            state: RelayState::Off,
        }
    }

    pub fn apply(&mut self, state: RelayState) {
        self.state = state;

        match state {
            RelayState::Off => {
                let _ = self.pin.set_low();
            }
            RelayState::On => {
                let _ = self.pin.set_high();
            }
        }
    }

    pub const fn state(&self) -> RelayState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use switcher_protocol::RelayState;

    #[test]
    fn active_high_semantics() {
        assert!(RelayState::On.as_bool());
        assert!(!RelayState::Off.as_bool());
    }
}
