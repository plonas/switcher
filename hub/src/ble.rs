use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use btleplug::{
    api::{
        Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, ValueNotification,
        WriteType,
    },
    platform::{Adapter, Manager, Peripheral},
};
use futures::StreamExt;
use switcher_protocol::{
    DeviceIdentity, HealthStatus, RelayCommand, RelayState, COMMAND_UUID, DEVICE_NAME_PREFIX,
    HEALTH_UUID, IDENTITY_UUID, SERVICE_UUID, STATE_UUID,
};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BleNotification {
    pub state: RelayState,
}

#[async_trait]
pub trait BleBridgeClient: Send + Sync {
    async fn connect(&self) -> Result<()>;
    async fn current_state(&self) -> Result<RelayState>;
    async fn send_command(&self, command: RelayCommand) -> Result<RelayState>;
    async fn health(&self) -> Result<HealthStatus>;
    async fn identity(&self) -> Result<DeviceIdentity>;
    fn subscribe(&self) -> broadcast::Receiver<BleNotification>;
}

#[derive(Clone)]
pub struct MockBleClient {
    inner: Arc<Mutex<MockBleState>>,
    tx: broadcast::Sender<BleNotification>,
}

#[derive(Clone)]
struct MockBleState {
    state: RelayState,
    health: HealthStatus,
    identity: DeviceIdentity,
}

impl Default for MockBleState {
    fn default() -> Self {
        Self {
            state: RelayState::Off,
            health: HealthStatus::new(1, 0, switcher_protocol::HealthCode::Ok),
            identity: DeviceIdentity::new(*b"dongle01", [0, 1, 0]),
        }
    }
}

impl MockBleClient {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(8);
        Self {
            inner: Arc::new(Mutex::new(MockBleState::default())),
            tx,
        }
    }
}

fn parse_uuid(input: &str) -> Result<Uuid> {
    Uuid::parse_str(input).with_context(|| format!("invalid UUID constant {input}"))
}

#[async_trait]
impl BleBridgeClient for MockBleClient {
    async fn connect(&self) -> Result<()> {
        Ok(())
    }

    async fn current_state(&self) -> Result<RelayState> {
        Ok(self.inner.lock().await.state)
    }

    async fn send_command(&self, command: RelayCommand) -> Result<RelayState> {
        let mut inner = self.inner.lock().await;
        inner.state = match command {
            RelayCommand::Off => RelayState::Off,
            RelayCommand::On => RelayState::On,
            RelayCommand::Toggle => inner.state.toggle(),
        };
        let _ = self.tx.send(BleNotification { state: inner.state });
        Ok(inner.state)
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(self.inner.lock().await.health.clone())
    }

    async fn identity(&self) -> Result<DeviceIdentity> {
        Ok(self.inner.lock().await.identity.clone())
    }

    fn subscribe(&self) -> broadcast::Receiver<BleNotification> {
        self.tx.subscribe()
    }
}

pub struct BtleplugClient {
    adapter: Adapter,
    peripheral: Mutex<Option<Peripheral>>,
    tx: broadcast::Sender<BleNotification>,
}

impl BtleplugClient {
    pub async fn new() -> Result<Self> {
        let manager = Manager::new().await?;
        let adapter = manager
            .adapters()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no BLE adapter found"))?;

        let (tx, _) = broadcast::channel(16);

        Ok(Self {
            adapter,
            peripheral: Mutex::new(None),
            tx,
        })
    }

    async fn ensure_peripheral(&self) -> Result<Peripheral> {
        if let Some(existing) = self.peripheral.lock().await.clone() {
            if existing.is_connected().await.unwrap_or(false) {
                return Ok(existing);
            }
        }

        self.adapter.start_scan(ScanFilter::default()).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;

        let peripherals = self.adapter.peripherals().await?;
        for peripheral in peripherals {
            let properties = peripheral.properties().await?;
            let Some(properties) = properties else {
                continue;
            };

            let matches_name = properties
                .local_name
                .as_deref()
                .map(|name| name.starts_with(DEVICE_NAME_PREFIX))
                .unwrap_or(false);

            let matches_service = properties
                .services
                .contains(&parse_uuid(SERVICE_UUID)?);

            if matches_name || matches_service {
                peripheral.connect().await?;
                peripheral.discover_services().await?;
                self.spawn_notifications(peripheral.clone()).await?;
                *self.peripheral.lock().await = Some(peripheral.clone());
                return Ok(peripheral);
            }
        }

        Err(anyhow!("no matching dongle found"))
    }

    async fn spawn_notifications(&self, peripheral: Peripheral) -> Result<()> {
        let state_char = self.find_characteristic(&peripheral, parse_uuid(STATE_UUID)?)?;
        if !state_char.properties.contains(CharPropFlags::NOTIFY) {
            return Ok(());
        }

        peripheral.subscribe(&state_char).await?;
        let mut notifications = peripheral.notifications().await?;
        let tx = self.tx.clone();

        tokio::spawn(async move {
            while let Some(ValueNotification { uuid, value }) = notifications.next().await {
                if uuid == parse_uuid(STATE_UUID).unwrap() {
                    if let Ok(state) = RelayState::try_from(value.as_slice()) {
                        let _ = tx.send(BleNotification { state });
                    }
                }
            }
        });

        Ok(())
    }

    fn find_characteristic(
        &self,
        peripheral: &Peripheral,
        uuid: Uuid,
    ) -> Result<btleplug::api::Characteristic> {
        peripheral
            .characteristics()
            .into_iter()
            .find(|characteristic| characteristic.uuid == uuid)
            .ok_or_else(|| anyhow!("missing characteristic {uuid}"))
    }

    async fn read_characteristic(&self, uuid: Uuid) -> Result<Vec<u8>> {
        let peripheral = self.ensure_peripheral().await?;
        let characteristic = self.find_characteristic(&peripheral, uuid)?;
        peripheral
            .read(&characteristic)
            .await
            .with_context(|| format!("failed to read characteristic {uuid}"))
    }

    async fn write_characteristic(&self, uuid: Uuid, payload: &[u8]) -> Result<()> {
        let peripheral = self.ensure_peripheral().await?;
        let characteristic = self.find_characteristic(&peripheral, uuid)?;
        peripheral
            .write(&characteristic, payload, WriteType::WithResponse)
            .await
            .with_context(|| format!("failed to write characteristic {uuid}"))
    }
}

#[async_trait]
impl BleBridgeClient for BtleplugClient {
    async fn connect(&self) -> Result<()> {
        let _ = self.ensure_peripheral().await?;
        Ok(())
    }

    async fn current_state(&self) -> Result<RelayState> {
        let value = self.read_characteristic(parse_uuid(STATE_UUID)?).await?;
        Ok(RelayState::try_from(value.as_slice())?)
    }

    async fn send_command(&self, command: RelayCommand) -> Result<RelayState> {
        self.write_characteristic(parse_uuid(COMMAND_UUID)?, &command.encode())
            .await?;
        self.current_state().await
    }

    async fn health(&self) -> Result<HealthStatus> {
        let value = self.read_characteristic(parse_uuid(HEALTH_UUID)?).await?;
        Ok(HealthStatus::try_from(value.as_slice())?)
    }

    async fn identity(&self) -> Result<DeviceIdentity> {
        let value = self.read_characteristic(parse_uuid(IDENTITY_UUID)?).await?;
        Ok(DeviceIdentity::try_from(value.as_slice())?)
    }

    fn subscribe(&self) -> broadcast::Receiver<BleNotification> {
        self.tx.subscribe()
    }
}

pub fn metadata_map(identity: &DeviceIdentity, health: &HealthStatus) -> HashMap<&'static str, String> {
    let mut map = HashMap::new();
    map.insert("device_id", format!("{:02x?}", identity.device_id));
    map.insert("firmware_version", format!("{}.{}.{}", identity.firmware_version[0], identity.firmware_version[1], identity.firmware_version[2]));
    map.insert("boot_count", health.boot_count.to_string());
    map
}
