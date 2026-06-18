use embassy_nrf::gpio::Output;
use switcher_protocol::RelayState;

pub struct RelayOutput<'d> {
    pin: Output<'d>,
    state: RelayState,
}

impl<'d> RelayOutput<'d> {
    pub fn new(pin: Output<'d>) -> Self {
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
