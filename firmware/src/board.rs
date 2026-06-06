use nrf52840_hal::{
    gpio::{
        p0::Parts as P0Parts,
        Level,
        Output,
        Pin,
        PushPull,
    },
    pac::P0,
};

use crate::relay::RelayOutput;

pub const RELAY_PIN: u8 = 13;

pub fn relay_from_p0(port: P0) -> RelayOutput {
    let p0 = P0Parts::new(port);
    let pin: Pin<Output<PushPull>> = p0.p0_13.into_push_pull_output(Level::Low).degrade();
    RelayOutput::new(pin)
}
