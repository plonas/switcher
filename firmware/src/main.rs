#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt::{info, trace};
use defmt_rtt as _;
use panic_probe as _;
use switcher_firmware::{
    FirmwareApp,
    ble::{FirmwareGattServer, GattCharacteristic, GattValue},
    board,
};

#[entry]
fn main() -> ! {
    let peripherals = nrf52840_hal::pac::Peripherals::take().unwrap();
    let _core = nrf52840_hal::pac::CorePeripherals::take().unwrap();
    board::ensure_regout0_is_3v0(&peripherals.NVMC, &peripherals.UICR);

    let mut app = FirmwareApp::new(*b"dongle01", [0, 1, 0]);
    let mut gatt = FirmwareGattServer::new();
    let (mut relay, mut status_led) = board::outputs_from_p0(peripherals.P0);
    relay.apply(app.relay_state());
    status_led.set(false);
    info!("switcher-firmware boot");
    info!("relay initialized on P0.13, state={:?}", relay.state());
    let snapshot = gatt.snapshot(&app);
    info!("BLE service ready on UUID {}", snapshot.service_uuid);
    trace!("state characteristic UUID {}", snapshot.state_uuid);
    trace!("command characteristic UUID {}", snapshot.command_uuid);

    loop {
        app.tick(1);
        if let Some(notification) = gatt.take_notification() {
            relay.apply(notification.state);
            status_led.set(notification.state.as_bool());
            trace!(
                "state notification payload={=[u8]:x}",
                notification.payload.as_slice()
            );
        }

        if let Ok(GattValue::State(payload)) = gatt.read(GattCharacteristic::State, &app) {
            trace!("state characteristic payload={=[u8]:x}", payload.as_slice());
        }
        trace!(
            "heartbeat uptime={}s relay={:?}",
            app.snapshot().uptime_seconds,
            relay.state()
        );
        cortex_m::asm::delay(16_000_000);
    }
}
