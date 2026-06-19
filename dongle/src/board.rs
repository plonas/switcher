use core::fmt::Write;

use embassy_nrf::Peri;
use embassy_nrf::gpio::{Level, Output, OutputDrive, Pin};
use heapless::String;
use switcher_protocol::DEVICE_NAME_PREFIX;

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

pub fn device_id() -> [u8; 8] {
    let low = embassy_nrf::pac::FICR.deviceid(0).read();
    let high = embassy_nrf::pac::FICR.deviceid(1).read();
    device_id_from_words(low, high)
}

pub fn advertised_name(device_id: &[u8; 8]) -> String<24> {
    let mut name = String::<24>::new();
    let _ = write!(name, "{DEVICE_NAME_PREFIX}-");

    for byte in &device_id[..4] {
        let _ = write!(name, "{byte:02x}");
    }

    name
}

fn device_id_from_words(low: u32, high: u32) -> [u8; 8] {
    let mut device_id = [0_u8; 8];
    device_id[..4].copy_from_slice(&low.to_le_bytes());
    device_id[4..].copy_from_slice(&high.to_le_bytes());
    device_id
}

pub fn outputs<'d>(
    relay_pin: Peri<'d, impl Pin>,
    led_pin: Peri<'d, impl Pin>,
) -> (RelayOutput<'d>, StatusLedOutput<'d>) {
    let relay = Output::new(relay_pin, Level::Low, OutputDrive::Standard);
    let led = Output::new(led_pin, Level::High, OutputDrive::Standard);

    (RelayOutput::new(relay), StatusLedOutput::new(led))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_name_uses_prefix_and_low_word_suffix() {
        let name = advertised_name(b"\x12\x34\x56\x78abcdef");
        assert_eq!(name.as_str(), "dongle-12345678");
    }

    #[test]
    fn device_id_words_are_encoded_little_endian() {
        assert_eq!(
            device_id_from_words(0x7856_3412, 0xf0de_bc9a),
            [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]
        );
    }
}
