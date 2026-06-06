#![no_std]
#![no_main]

use cortex_m::asm::wfi;
use cortex_m_rt::entry;
use panic_halt as _;
use switcher_firmware::{board, FirmwareApp};

#[entry]
fn main() -> ! {
    let peripherals = nrf52840_hal::pac::Peripherals::take().unwrap();
    let _core = nrf52840_hal::pac::CorePeripherals::take().unwrap();

    let mut app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
    let mut relay = board::relay_from_p0(peripherals.P0);
    relay.apply(app.relay_state());

    loop {
        app.tick(1);
        wfi();
    }
}
