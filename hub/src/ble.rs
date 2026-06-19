use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use btleplug::{
    api::{
        Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, ValueNotification,
        WriteType,
    },
    platform::{Adapter, Manager, Peripheral},
};
use futures::StreamExt;
use log::debug;
use switcher_protocol::{
    COMMAND_UUID, DEVICE_NAME_PREFIX, DeviceIdentity, HEALTH_UUID, HealthStatus, IDENTITY_UUID,
    RelayCommand, RelayState, SERVICE_UUID, STATE_UUID,
};
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use crate::device_id::format_device_id;
#[cfg(test)]
use crate::device_id::parse_device_id;

#[derive(Debug, Clone)]
pub struct BleNotification {
    pub state: RelayState,
}

#[async_trait]
pub trait BleBridgeClient: Send + Sync {
    async fn connect(&self) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn current_state(&self) -> Result<RelayState>;
    async fn send_command(&self, command: RelayCommand) -> Result<RelayState>;
    async fn health(&self) -> Result<HealthStatus>;
    async fn identity(&self) -> Result<DeviceIdentity>;
    async fn address(&self) -> Result<Option<String>>;
    fn subscribe(&self) -> broadcast::Receiver<BleNotification>;
}

#[derive(Clone)]
pub struct MockBleClient {
    inner: Arc<Mutex<MockBleState>>,
    tx: broadcast::Sender<BleNotification>,
}

#[derive(Clone)]
struct MockBleState {
    connected: bool,
    fail_connect_once: bool,
    address: Option<String>,
    state: RelayState,
    health: HealthStatus,
    identity: DeviceIdentity,
}

impl Default for MockBleState {
    fn default() -> Self {
        Self {
            connected: false,
            fail_connect_once: false,
            address: Some("AA:BB:CC:DD:EE:FF".into()),
            state: RelayState::Off,
            health: HealthStatus::new(1, 0, switcher_protocol::HealthCode::Ok),
            identity: DeviceIdentity::new(*b"dongle01", [0, 1, 0]),
        }
    }
}

impl MockBleClient {
    pub fn new() -> Self {
        Self::with_device(*b"dongle01", Some("AA:BB:CC:DD:EE:FF".into()))
    }

    pub fn with_device(device_id: [u8; 8], address: Option<String>) -> Self {
        let (tx, _) = broadcast::channel(8);
        Self {
            inner: Arc::new(Mutex::new(MockBleState {
                address,
                identity: DeviceIdentity::new(device_id, [0, 1, 0]),
                ..MockBleState::default()
            })),
            tx,
        }
    }

    pub async fn disconnect(&self) {
        self.inner.lock().await.connected = false;
    }

    pub async fn fail_next_connect(&self) {
        self.inner.lock().await.fail_connect_once = true;
    }

    async fn ensure_connected(&self) -> Result<()> {
        if self.inner.lock().await.connected {
            Ok(())
        } else {
            Err(anyhow!("mock BLE client is disconnected"))
        }
    }
}

fn parse_uuid(input: &str) -> Result<Uuid> {
    Uuid::parse_str(input).with_context(|| format!("invalid UUID constant {input}"))
}

#[async_trait]
impl BleBridgeClient for MockBleClient {
    async fn connect(&self) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if inner.fail_connect_once {
            inner.fail_connect_once = false;
            inner.connected = false;
            return Err(anyhow!("mock BLE connect failed"));
        }
        inner.connected = true;
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.inner.lock().await.connected = false;
        Ok(())
    }

    async fn current_state(&self) -> Result<RelayState> {
        self.ensure_connected().await?;
        Ok(self.inner.lock().await.state)
    }

    async fn send_command(&self, command: RelayCommand) -> Result<RelayState> {
        self.ensure_connected().await?;
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
        self.ensure_connected().await?;
        Ok(self.inner.lock().await.health.clone())
    }

    async fn identity(&self) -> Result<DeviceIdentity> {
        self.ensure_connected().await?;
        Ok(self.inner.lock().await.identity.clone())
    }

    async fn address(&self) -> Result<Option<String>> {
        Ok(self.inner.lock().await.address.clone())
    }

    fn subscribe(&self) -> broadcast::Receiver<BleNotification> {
        self.tx.subscribe()
    }
}

pub struct BtleplugManager {
    adapter: Adapter,
    discovery_lock: Mutex<()>,
    claimed_addresses: Mutex<HashMap<String, [u8; 8]>>,
}

impl BtleplugManager {
    pub async fn new() -> Result<Arc<Self>> {
        let manager = Manager::new().await?;
        let adapter = manager
            .adapters()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no BLE adapter found"))?;

        Ok(Arc::new(Self {
            adapter,
            discovery_lock: Mutex::new(()),
            claimed_addresses: Mutex::new(HashMap::new()),
        }))
    }

    async fn is_claimed_by_other(
        &self,
        address: &str,
        expected_device_id: Option<[u8; 8]>,
    ) -> bool {
        let claims = self.claimed_addresses.lock().await;
        match claims.get(address) {
            Some(owner) => Some(*owner) != expected_device_id,
            None => false,
        }
    }

    async fn claim(&self, address: String, expected_device_id: Option<[u8; 8]>) {
        if let Some(device_id) = expected_device_id {
            self.claimed_addresses.lock().await.insert(address, device_id);
        }
    }

    async fn unclaim(&self, address: &str, expected_device_id: Option<[u8; 8]>) {
        let Some(device_id) = expected_device_id else {
            return;
        };

        let mut claims = self.claimed_addresses.lock().await;
        if claims.get(address) == Some(&device_id) {
            claims.remove(address);
        }
    }
}

pub struct BtleplugClient {
    shared: Arc<BtleplugManager>,
    expected_device_id: Option<[u8; 8]>,
    preferred_address: Option<String>,
    cached_address: Mutex<Option<String>>,
    current_address: Mutex<Option<String>>,
    peripheral: Mutex<Option<Peripheral>>,
    tx: broadcast::Sender<BleNotification>,
}

impl BtleplugClient {
    pub fn new(
        shared: Arc<BtleplugManager>,
        expected_device_id: Option<[u8; 8]>,
        preferred_address: Option<String>,
        cached_address: Option<String>,
    ) -> Self {
        let (tx, _) = broadcast::channel(16);

        Self {
            shared,
            expected_device_id,
            preferred_address: preferred_address.and_then(|value| normalize_ble_address(&value)),
            cached_address: Mutex::new(cached_address.and_then(|value| normalize_ble_address(&value))),
            current_address: Mutex::new(None),
            peripheral: Mutex::new(None),
            tx,
        }
    }

    async fn ensure_peripheral(&self) -> Result<Peripheral> {
        if let Some(existing) = self.peripheral.lock().await.clone() {
            if existing.is_connected().await.unwrap_or(false) {
                return Ok(existing);
            }
        }

        let _scan_guard = self.shared.discovery_lock.lock().await;
        self.shared.adapter.start_scan(ScanFilter::default()).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;

        let peripherals = self.shared.adapter.peripherals().await?;
        let preferred = self.preferred_address.clone();
        let cached = self.cached_address.lock().await.clone();

        for address in candidate_addresses(preferred.as_deref(), cached.as_deref()) {
            if let Some(peripheral) = self.find_by_address(&peripherals, &address).await? {
                if let Some(peripheral) = self.try_connect_matching(peripheral).await? {
                    return Ok(peripheral);
                }
            }
        }

        let mut seen = HashSet::new();
        for peripheral in &peripherals {
            let properties = peripheral.properties().await?;
            let Some(properties) = properties else {
                continue;
            };

            let address = properties.address.to_string();
            seen.insert(address.clone());

            let matches_name = properties
                .local_name
                .as_deref()
                .map(|name| name.starts_with(DEVICE_NAME_PREFIX))
                .unwrap_or(false);
            let matches_service = properties.services.contains(&parse_uuid(SERVICE_UUID)?);

            if (matches_name || matches_service)
                && let Some(peripheral) = self.try_connect_matching(peripheral.clone()).await?
            {
                return Ok(peripheral);
            }
        }

        for peripheral in peripherals {
            let Some(properties) = peripheral.properties().await? else {
                continue;
            };
            let address = properties.address.to_string();
            if seen.contains(&address) {
                continue;
            }

            if let Some(peripheral) = self.try_connect_matching(peripheral).await? {
                return Ok(peripheral);
            }
        }

        Err(anyhow!(
            "no matching dongle found{}",
            self.expected_device_id
                .map(|device_id| format!(" for device_id {}", format_device_id(&device_id)))
                .unwrap_or_default()
        ))
    }

    async fn find_by_address(
        &self,
        peripherals: &[Peripheral],
        address: &str,
    ) -> Result<Option<Peripheral>> {
        for peripheral in peripherals {
            let properties = peripheral.properties().await?;
            let Some(properties) = properties else {
                continue;
            };

            if properties.address.to_string().eq_ignore_ascii_case(address) {
                return Ok(Some(peripheral.clone()));
            }
        }

        Ok(None)
    }

    async fn try_connect_matching(&self, peripheral: Peripheral) -> Result<Option<Peripheral>> {
        let raw_address = peripheral
            .properties()
            .await?
            .map(|properties| properties.address.to_string());
        let address = raw_address.as_deref().and_then(normalize_ble_address);

        if let Some(address) = address.as_deref()
            && self
                .shared
                .is_claimed_by_other(address, self.expected_device_id)
                .await
        {
            return Ok(None);
        }

        let connected = match self.connect_peripheral(peripheral.clone()).await {
            Ok(peripheral) => peripheral,
            Err(error) => {
                debug!("BLE connect skipped for {:?}: {error}", peripheral.id());
                return Ok(None);
            }
        };

        if !self.matches_connected_dongle(&connected) {
            let _ = connected.disconnect().await;
            return Ok(None);
        }

        let identity = match self.read_identity_from(&connected).await {
            Ok(identity) => identity,
            Err(error) => {
                debug!("BLE identity probe failed for {:?}: {error}", connected.id());
                let _ = connected.disconnect().await;
                return Ok(None);
            }
        };

        if let Some(expected_device_id) = self.expected_device_id
            && identity.device_id != expected_device_id
        {
            debug!(
                "BLE device_id mismatch: expected {}, found {}",
                format_device_id(&expected_device_id),
                format_device_id(&identity.device_id)
            );
            let _ = connected.disconnect().await;
            return Ok(None);
        }

        self.spawn_notifications(connected.clone()).await?;
        if let Some(address) = address {
            self.shared
                .claim(address.clone(), self.expected_device_id)
                .await;
            *self.current_address.lock().await = Some(address.clone());
            *self.cached_address.lock().await = Some(address);
        }
        *self.peripheral.lock().await = Some(connected.clone());

        Ok(Some(connected))
    }

    async fn connect_peripheral(&self, peripheral: Peripheral) -> Result<Peripheral> {
        if !peripheral.is_connected().await.unwrap_or(false) {
            peripheral.connect().await?;
        }
        peripheral.discover_services().await?;
        Ok(peripheral)
    }

    async fn clear_peripheral(&self, disconnect: bool) -> Result<()> {
        let previous = self.peripheral.lock().await.take();
        let current_address = self.current_address.lock().await.take();

        if let Some(address) = current_address {
            self.shared.unclaim(&address, self.expected_device_id).await;
        }

        if disconnect && let Some(peripheral) = previous {
            let _ = peripheral.disconnect().await;
        }

        Ok(())
    }

    fn matches_connected_dongle(&self, peripheral: &Peripheral) -> bool {
        let service_uuid = parse_uuid(SERVICE_UUID).ok();
        let state_uuid = parse_uuid(STATE_UUID).ok();
        let command_uuid = parse_uuid(COMMAND_UUID).ok();
        let health_uuid = parse_uuid(HEALTH_UUID).ok();
        let identity_uuid = parse_uuid(IDENTITY_UUID).ok();

        let services = peripheral.services();
        let has_service = service_uuid
            .as_ref()
            .is_some_and(|uuid| services.iter().any(|service| service.uuid == *uuid));

        let characteristics = peripheral.characteristics();
        let has_state = state_uuid.as_ref().is_some_and(|uuid| {
            characteristics
                .iter()
                .any(|characteristic| characteristic.uuid == *uuid)
        });
        let has_command = command_uuid.as_ref().is_some_and(|uuid| {
            characteristics
                .iter()
                .any(|characteristic| characteristic.uuid == *uuid)
        });
        let has_health = health_uuid.as_ref().is_some_and(|uuid| {
            characteristics
                .iter()
                .any(|characteristic| characteristic.uuid == *uuid)
        });
        let has_identity = identity_uuid.as_ref().is_some_and(|uuid| {
            characteristics
                .iter()
                .any(|characteristic| characteristic.uuid == *uuid)
        });

        has_service || (has_state && has_command && has_health && has_identity)
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
            while let Some(ValueNotification { uuid, value, .. }) = notifications.next().await {
                if uuid == parse_uuid(STATE_UUID).unwrap()
                    && let Ok(state) = RelayState::try_from(value.as_slice())
                {
                    let _ = tx.send(BleNotification { state });
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

    async fn read_identity_from(&self, peripheral: &Peripheral) -> Result<DeviceIdentity> {
        let characteristic = self.find_characteristic(peripheral, parse_uuid(IDENTITY_UUID)?)?;
        let value = peripheral
            .read(&characteristic)
            .await
            .context("failed to read identity characteristic while probing peripheral")?;
        Ok(DeviceIdentity::try_from(
            decode_exact_or_prefix::<{ DeviceIdentity::ENCODED_LEN }>(&value, "identity")?,
        )?)
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

fn decode_exact_or_prefix<'a, const N: usize>(value: &'a [u8], label: &str) -> Result<&'a [u8]> {
    if value.len() < N {
        return Err(anyhow!(
            "{label} payload too short: expected at least {N} bytes, got {}",
            value.len()
        ));
    }

    if value.len() > N {
        debug!(
            "{label} payload had trailing bytes: expected {N}, got {}, decoding prefix",
            value.len()
        );
    }

    Ok(&value[..N])
}

#[async_trait]
impl BleBridgeClient for BtleplugClient {
    async fn connect(&self) -> Result<()> {
        let _ = self.ensure_peripheral().await?;
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        self.clear_peripheral(false).await
    }

    async fn current_state(&self) -> Result<RelayState> {
        let value = self.read_characteristic(parse_uuid(STATE_UUID)?).await?;
        Ok(RelayState::try_from(decode_exact_or_prefix::<1>(&value, "state")?)?)
    }

    async fn send_command(&self, command: RelayCommand) -> Result<RelayState> {
        self.write_characteristic(parse_uuid(COMMAND_UUID)?, &command.encode())
            .await?;
        self.current_state().await
    }

    async fn health(&self) -> Result<HealthStatus> {
        let value = self.read_characteristic(parse_uuid(HEALTH_UUID)?).await?;
        Ok(HealthStatus::try_from(decode_exact_or_prefix::<{ HealthStatus::ENCODED_LEN }>(
            &value,
            "health",
        )?)?)
    }

    async fn identity(&self) -> Result<DeviceIdentity> {
        let value = self.read_characteristic(parse_uuid(IDENTITY_UUID)?).await?;
        Ok(DeviceIdentity::try_from(
            decode_exact_or_prefix::<{ DeviceIdentity::ENCODED_LEN }>(&value, "identity")?,
        )?)
    }

    async fn address(&self) -> Result<Option<String>> {
        Ok(self.current_address.lock().await.clone())
    }

    fn subscribe(&self) -> broadcast::Receiver<BleNotification> {
        self.tx.subscribe()
    }
}

pub fn metadata_map(
    identity: &DeviceIdentity,
    health: &HealthStatus,
) -> HashMap<&'static str, String> {
    let mut map = HashMap::new();
    map.insert("device_id", format_device_id(&identity.device_id));
    map.insert(
        "firmware_version",
        format!(
            "{}.{}.{}",
            identity.firmware_version[0],
            identity.firmware_version[1],
            identity.firmware_version[2]
        ),
    );
    map.insert("boot_count", health.boot_count.to_string());
    map
}

fn candidate_addresses(preferred: Option<&str>, cached: Option<&str>) -> Vec<String> {
    let mut addresses = Vec::new();

    if let Some(address) = preferred.and_then(normalize_ble_address) {
        addresses.push(address);
    }

    if let Some(address) = cached.and_then(normalize_ble_address)
        && !addresses
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&address))
    {
        addresses.push(address);
    }

    addresses
}

pub(crate) fn normalize_ble_address(address: &str) -> Option<String> {
    let trimmed = address.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("00:00:00:00:00:00") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_addresses_prioritizes_preferred_then_cached() {
        assert_eq!(
            candidate_addresses(Some("AA:BB:CC:DD:EE:FF"), Some("11:22:33:44:55:66")),
            vec![
                "AA:BB:CC:DD:EE:FF".to_string(),
                "11:22:33:44:55:66".to_string()
            ]
        );
    }

    #[test]
    fn candidate_addresses_deduplicates() {
        assert_eq!(
            candidate_addresses(Some("AA:BB:CC:DD:EE:FF"), Some("aa:bb:cc:dd:ee:ff")),
            vec!["AA:BB:CC:DD:EE:FF".to_string()]
        );
    }

    #[tokio::test]
    #[ignore = "requires a physical dongle advertising over BLE"]
    async fn btleplug_client_can_read_live_dongle() {
        let expected = std::env::var("DONGLE_DEVICE_ID")
            .ok()
            .map(|value| parse_device_id(&value).unwrap());
        let preferred = std::env::var("DONGLE_BLE_ADDRESS").ok();
        let shared = BtleplugManager::new().await.unwrap();
        let client = BtleplugClient::new(shared, expected, preferred, None);
        client.connect().await.unwrap();
        let _ = client.identity().await.unwrap();
        let _ = client.health().await.unwrap();
        let _ = client.current_state().await.unwrap();
    }
}
