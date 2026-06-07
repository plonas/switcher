#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt::{    
    info, trace,
};
use defmt_rtt as _;
use panic_probe as _;
use switcher_protocol::RelayCommand;
use switcher_firmware::{board, FirmwareApp};

#[entry]
fn main() -> ! {
    let peripherals = nrf52840_hal::pac::Peripherals::take().unwrap();
    let _core = nrf52840_hal::pac::CorePeripherals::take().unwrap();
    board::ensure_regout0_is_3v0(&peripherals.NVMC, &peripherals.UICR);

    let mut app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
    let (mut relay, mut status_led) = board::outputs_from_p0(peripherals.P0);
    relay.apply(app.relay_state());
    status_led.set(false);
    info!("switcher-firmware boot");
    info!("relay initialized on P0.13, state={:?}", relay.state());

    loop {
        app.tick(1);
        let next_state = app.apply_command(RelayCommand::Toggle);
        relay.apply(next_state);
        status_led.set(next_state.as_bool());
        trace!(
            "heartbeat uptime={}s relay={:?}",
            app.snapshot().uptime_seconds,
            relay.state()
        );
        cortex_m::asm::delay(16_000_000);
    }
}
