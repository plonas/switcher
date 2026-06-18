use embassy_nrf::Peri;
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pin};

use crate::relay::RelayOutput;

pub const RELAY_PIN: u8 = 13;
pub const STATUS_LED_PIN: u8 = 8;

pub struct StatusLedOutput<'d> {
    pin: Output<'d>,
    is_on: bool,
}

impl<'d> StatusLedOutput<'d> {
    pub fn new(pin: Output<'d>) -> Self {
        Self { pin, is_on: false }
    }

    pub fn set(&mut self, on: bool) {
        self.is_on = on;

        if on {
            let _ = self.pin.set_low();
        } else {
            let _ = self.pin.set_high();
        }
    }
}

pub fn ensure_regout0_is_3v0() {
    // Embassy config programs REGOUT0 for us during HAL init.
}

pub fn outputs<'d>(
    relay_pin: Peri<'d, impl Pin>,
    led_pin: Peri<'d, impl Pin>,
) -> (RelayOutput<'d>, StatusLedOutput<'d>) {
    let relay = Output::new(relay_pin, Level::Low, OutputDrive::Standard);
    let led = Output::new(led_pin, Level::High, OutputDrive::Standard);

    (RelayOutput::new(relay), StatusLedOutput::new(led))
}
