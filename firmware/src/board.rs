use embedded_hal::digital::OutputPin;
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
use nrf52840_hal::pac::uicr::regout0::VOUT_A;
use nrf52840_hal::pac::{NVMC, UICR};

use crate::relay::RelayOutput;

pub const RELAY_PIN: u8 = 13;
pub const STATUS_LED_PIN: u8 = 8;

pub struct StatusLedOutput {
    pin: Pin<Output<PushPull>>,
    is_on: bool,
}

impl StatusLedOutput {
    pub fn new(pin: Pin<Output<PushPull>>) -> Self {
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

pub fn ensure_regout0_is_3v0(nvmc: &NVMC, uicr: &UICR) {
    let current_vout = uicr.regout0.read().vout().variant();

    if matches!(current_vout, Some(VOUT_A::_3V0)) {
        return;
    }

    while nvmc.ready.read().ready().is_busy() {}

    if !matches!(current_vout, Some(VOUT_A::DEFAULT)) {
        nvmc.config.write(|w| w.wen().een());
        while nvmc.ready.read().ready().is_busy() {}

        nvmc.eraseuicr.write(|w| w.eraseuicr().erase());
        while nvmc.ready.read().ready().is_busy() {}
    }

    nvmc.config.write(|w| w.wen().wen());
    while nvmc.ready.read().ready().is_busy() {}

    uicr.regout0.write(|w| w.vout()._3v0());
    while nvmc.ready.read().ready().is_busy() {}

    nvmc.config.write(|w| w.wen().ren());
    while nvmc.ready.read().ready().is_busy() {}

    cortex_m::peripheral::SCB::sys_reset();
}

pub fn relay_from_p0(port: P0) -> RelayOutput {
    let p0 = P0Parts::new(port);
    let pin: Pin<Output<PushPull>> = p0.p0_13.into_push_pull_output(Level::Low).degrade();
    RelayOutput::new(pin)
}

pub fn outputs_from_p0(port: P0) -> (RelayOutput, StatusLedOutput) {
    let p0 = P0Parts::new(port);
    let relay_pin: Pin<Output<PushPull>> =
        p0.p0_13.into_push_pull_output(Level::Low).degrade();
    let led_pin: Pin<Output<PushPull>> =
        p0.p0_08.into_push_pull_output(Level::High).degrade();

    (RelayOutput::new(relay_pin), StatusLedOutput::new(led_pin))
}
