use defmt::{debug, info, warn};
use embassy_futures::join::join;
use embassy_time::Instant;
use switcher_protocol::{
    COMMAND_UUID, DEVICE_NAME_PREFIX, HEALTH_UUID, HealthCode, IDENTITY_UUID, SERVICE_UUID,
    STATE_UUID,
};
use trouble_host::prelude::*;

use crate::{
    FirmwareApp,
    ble::{FirmwareGattServer, GattCharacteristic, GattValue},
    board::StatusLedOutput,
    relay::RelayOutput,
};

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;
const DEVICE_NAME: &str = "dongle01";
const SERVICE_UUID_BYTES_LE: [u8; 16] = [
    0x55, 0x44, 0x33, 0x22, 0x11, 0x00, 0x00, 0x81, 0x00, 0x49, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

#[gatt_server]
struct SwitcherServer {
    switcher: SwitcherService,
}

#[gatt_service(uuid = "0f0e0d0c-0b0a-4900-8100-001122334455")]
struct SwitcherService {
    #[characteristic(
        uuid = "0f0e0d0c-0b0a-4900-8101-001122334455",
        read,
        notify,
        value = 0u8
    )]
    state: u8,
    #[characteristic(uuid = "0f0e0d0c-0b0a-4900-8102-001122334455", write)]
    command: (),
    #[characteristic(uuid = "0f0e0d0c-0b0a-4900-8103-001122334455", read)]
    health: (),
    #[characteristic(uuid = "0f0e0d0c-0b0a-4900-8104-001122334455", read)]
    identity: (),
}

pub async fn run<C>(controller: C, mut relay: RelayOutput<'_>, mut status_led: StatusLedOutput<'_>)
where
    C: Controller,
{
    let address = Address::random([0xff, 0x01, 0x02, 0x03, 0x04, 0x05]);
    let mut resources: HostResources<_, DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .build();
    let runner = stack.runner();
    let mut peripheral = stack.peripheral();

    let mut app = FirmwareApp::new(*b"dongle01", [0, 2, 0]);
    let mut gatt = FirmwareGattServer::new();
    let boot_at = Instant::now();

    relay.apply(app.relay_state());
    status_led.set(false);
    info!("dongle boot");
    info!("device name prefix={}", DEVICE_NAME_PREFIX);
    info!("BLE service UUID {}", SERVICE_UUID);
    debug!("state UUID {}", STATE_UUID);
    debug!("command UUID {}", COMMAND_UUID);
    debug!("health UUID {}", HEALTH_UUID);
    debug!("identity UUID {}", IDENTITY_UUID);

    let _ = join(ble_task(runner), async {
        loop {
            let server = SwitcherServer::new_with_config(GapConfig::Peripheral(PeripheralConfig {
                name: DEVICE_NAME,
                appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
            }))
            .unwrap();

            sync_runtime(&mut app, &mut relay, &mut status_led, boot_at);

            match advertise(&mut peripheral, &server).await {
                Ok(conn) => {
                    if let Err(error) = gatt_events_task(
                        &server,
                        &conn,
                        &mut app,
                        &mut gatt,
                        &mut relay,
                        &mut status_led,
                        boot_at,
                    )
                    .await
                    {
                        warn!("[gatt] connection loop error: {:?}", error);
                        app.set_last_error(HealthCode::InternalError);
                    } else {
                        app.set_last_error(HealthCode::Ok);
                    }
                }
                Err(error) => {
                    warn!("[adv] advertise failed: {:?}", error);
                    app.set_last_error(HealthCode::InternalError);
                }
            }
        }
    })
    .await;
}

async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if let Err(error) = runner.run().await {
            warn!("[ble_task] error: {:?}", error);
        }
    }
}

async fn advertise<'values, 'server, C: Controller>(
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server SwitcherServer<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv_data = [0; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteServiceUuids128(&[SERVICE_UUID_BYTES_LE]),
        ],
        &mut adv_data,
    )?;

    let mut scan_data = [0; 31];
    let scan_len = AdStructure::encode_slice(
        &[AdStructure::CompleteLocalName(DEVICE_NAME.as_bytes())],
        &mut scan_data,
    )?;

    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..adv_len],
                scan_data: &scan_data[..scan_len],
            },
        )
        .await?;
    info!("[adv] advertising as {}", DEVICE_NAME);
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    info!("[adv] connection established");
    Ok(conn)
}

async fn gatt_events_task<P: PacketPool>(
    server: &SwitcherServer<'_>,
    conn: &GattConnection<'_, '_, P>,
    app: &mut FirmwareApp,
    gatt: &mut FirmwareGattServer,
    relay: &mut RelayOutput<'_>,
    status_led: &mut StatusLedOutput<'_>,
    boot_at: Instant,
) -> Result<(), Error> {
    let state = server.switcher.state;
    let command_handle = server.switcher.command.handle;
    let health_handle = server.switcher.health.handle;
    let identity_handle = server.switcher.identity.handle;

    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                warn!("[gatt] disconnected: {:?}", reason);
                app.set_last_error(HealthCode::BleDisconnected);
                break;
            }
            GattConnectionEvent::Gatt { event } => {
                sync_runtime(app, relay, status_led, boot_at);
                let reply = match event {
                    GattEvent::Read(event) if event.handle() == state.handle => {
                        match gatt.read(GattCharacteristic::State, app) {
                            Ok(GattValue::State(payload)) => event.accept_unprocessed(&payload),
                            Err(error) => event.reject(map_read_error(error)),
                            _ => event.reject(AttErrorCode::UNLIKELY_ERROR),
                        }
                    }
                    GattEvent::Read(event) if event.handle() == health_handle => {
                        match gatt.read(GattCharacteristic::Health, app) {
                            Ok(GattValue::Health(payload)) => event.accept_unprocessed(&payload),
                            Err(error) => event.reject(map_read_error(error)),
                            _ => event.reject(AttErrorCode::UNLIKELY_ERROR),
                        }
                    }
                    GattEvent::Read(event) if event.handle() == identity_handle => {
                        match gatt.read(GattCharacteristic::Identity, app) {
                            Ok(GattValue::Identity(payload)) => event.accept_unprocessed(&payload),
                            Err(error) => event.reject(map_read_error(error)),
                            _ => event.reject(AttErrorCode::UNLIKELY_ERROR),
                        }
                    }
                    GattEvent::Write(event) if event.handle() == command_handle => {
                        let mut payload = [0_u8; 8];
                        let mut len = 0;
                        event.with_data(|offset, data| {
                            if offset == 0 && data.len() <= payload.len() {
                                payload[..data.len()].copy_from_slice(data);
                                len = data.len();
                            }
                        });

                        match gatt.write(GattCharacteristic::Command, &payload[..len], app) {
                            Ok(next_state) => {
                                relay.apply(next_state);
                                status_led.set(next_state.as_bool());
                                if let Some(notification) = gatt.take_notification() {
                                    if let Err(error) =
                                        state.notify(conn, &notification.payload[0], true).await
                                    {
                                        warn!("[gatt] state notify failed: {:?}", error);
                                        app.set_last_error(HealthCode::InternalError);
                                    }
                                }
                                event.accept_unprocessed()
                            }
                            Err(error) => event.reject(map_write_error(error)),
                        }
                    }
                    other => other.accept(),
                };

                match reply {
                    Ok(reply) => reply.send().await,
                    Err(error) => {
                        warn!("[gatt] reply error: {:?}", error);
                        app.set_last_error(HealthCode::InternalError);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn sync_runtime(
    app: &mut FirmwareApp,
    relay: &mut RelayOutput<'_>,
    status_led: &mut StatusLedOutput<'_>,
    boot_at: Instant,
) {
    let uptime = Instant::now().saturating_duration_since(boot_at).as_secs() as u32;
    app.set_uptime_seconds(uptime);
    relay.apply(app.relay_state());
    status_led.set(app.relay_state().as_bool());
}

fn map_read_error(error: crate::ble::GattAccessError) -> AttErrorCode {
    match error {
        crate::ble::GattAccessError::NotReadable(_) => AttErrorCode::READ_NOT_PERMITTED,
        crate::ble::GattAccessError::NotWritable(_) => AttErrorCode::WRITE_NOT_PERMITTED,
        crate::ble::GattAccessError::Protocol(_) => AttErrorCode::VALUE_NOT_ALLOWED,
    }
}

fn map_write_error(error: crate::ble::GattAccessError) -> AttErrorCode {
    match error {
        crate::ble::GattAccessError::NotReadable(_) => AttErrorCode::READ_NOT_PERMITTED,
        crate::ble::GattAccessError::NotWritable(_) => AttErrorCode::WRITE_NOT_PERMITTED,
        crate::ble::GattAccessError::Protocol(_) => AttErrorCode::VALUE_NOT_ALLOWED,
    }
}
