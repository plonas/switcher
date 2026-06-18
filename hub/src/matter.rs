#[cfg(feature = "matter")]
use core::pin::pin;
#[cfg(not(feature = "matter"))]
use std::sync::Arc;
#[cfg(feature = "matter")]
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[cfg(feature = "matter")]
use anyhow::Result;
#[cfg(not(feature = "matter"))]
use anyhow::{Result, bail};
#[cfg(feature = "matter")]
use embassy_futures::select::select4;
#[cfg(feature = "matter")]
use log::{error, info};
#[cfg(feature = "matter")]
use rs_matter::crypto::{Crypto, RngCore, default_crypto};
#[cfg(feature = "matter")]
use rs_matter::dm::IMBuffer;
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::app::level_control::LevelControlHooks;
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::app::on_off::{
    self, EffectVariantEnum, OnOffHandler, OnOffHooks, OutOfBandMessage, StartUpOnOffEnum,
};
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::basic_info::{BasicInfoConfig, PairingHintFlags};
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::decl::on_off as on_off_cluster;
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::groups::{self, ClusterHandler as _};
#[cfg(feature = "matter")]
use rs_matter::dm::clusters::net_comm::SharedNetworks;
#[cfg(feature = "matter")]
use rs_matter::dm::devices::DEV_TYPE_ON_OFF_LIGHT;
#[cfg(feature = "matter")]
use rs_matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM};
#[cfg(feature = "matter")]
use rs_matter::dm::endpoints;
#[cfg(feature = "matter")]
use rs_matter::dm::events::NoEvents;
#[cfg(feature = "matter")]
use rs_matter::dm::networks::SysNetifs;
#[cfg(feature = "matter")]
use rs_matter::dm::networks::eth::EthNetwork;
#[cfg(feature = "matter")]
use rs_matter::dm::subscriptions::Subscriptions;
#[cfg(feature = "matter")]
use rs_matter::dm::{Async, DataModel, DataModelHandler, Dataver, Endpoint, EpClMatcher, Node};
#[cfg(feature = "matter")]
use rs_matter::error::Error;
#[cfg(feature = "matter")]
use rs_matter::pairing::DiscoveryCapabilities;
#[cfg(feature = "matter")]
use rs_matter::pairing::qr::QrTextType;
#[cfg(feature = "matter")]
use rs_matter::persist::{DirKvBlobStore, SharedKvBlobStore};
#[cfg(feature = "matter")]
use rs_matter::respond::DefaultResponder;
#[cfg(feature = "matter")]
use rs_matter::sc::pase::MAX_COMM_WINDOW_TIMEOUT_SECS;
#[cfg(feature = "matter")]
use rs_matter::tlv::Nullable;
#[cfg(feature = "matter")]
use rs_matter::transport::MATTER_SOCKET_BIND_ADDR;
#[cfg(feature = "matter")]
use rs_matter::transport::network::mdns::zeroconf::ZeroconfMdnsResponder;
#[cfg(feature = "matter")]
use rs_matter::utils::select::Coalesce;
#[cfg(feature = "matter")]
use rs_matter::utils::storage::pooled::PooledBuffers;
#[cfg(feature = "matter")]
use rs_matter::{Matter, clusters, devices, root_endpoint, with};
#[cfg(feature = "matter")]
use tokio::sync::{mpsc, watch};

#[cfg(not(feature = "matter"))]
use crate::{ble::BleBridgeClient, bridge::RelayBridge};
#[cfg(feature = "matter")]
use crate::{
    ble::BleBridgeClient,
    bridge::{BridgeStatus, RelayBridge},
};

#[cfg(feature = "matter")]
const BASIC_INFO: BasicInfoConfig<'static> = BasicInfoConfig {
    vendor_name: "Dongle",
    vid: 0xFFF1,
    product_name: "Dongle Hub",
    pid: 0x8000,
    hw_ver: 1,
    hw_ver_str: "1",
    sw_ver: 1,
    sw_ver_str: "0.1.0",
    manufacturing_date: "",
    part_number: "",
    product_url: "",
    product_label: "Dongle BLE Bridge",
    serial_no: "dongle-bbb-001",
    unique_id: "dongle-bbb-001",
    capability_minima: rs_matter::dm::clusters::basic_info::CapabilityMinima::new(),
    product_appearance: rs_matter::dm::clusters::basic_info::ProductAppearance::new(),
    specification_version: rs_matter::dm::clusters::basic_info::DEFAULT_MATTER_SPEC_VERSION,
    data_model_revision: rs_matter::dm::clusters::basic_info::DEFAULT_DATA_MODEL_REVISION,
    max_paths_per_invoke: rs_matter::dm::clusters::basic_info::DEFAULT_MAX_PATHS_PER_INVOKE,
    device_name: "Dongle Hub",
    device_type: Some(0x0100),
    pairing_hint: PairingHintFlags::empty(),
    pairing_instruction: "",
    sai: None,
    sii: None,
    tcp_supported: false,
};

#[derive(Debug, Clone, Copy)]
pub struct MatterNodeConfig {
    pub port: u16,
}

impl Default for MatterNodeConfig {
    fn default() -> Self {
        Self { port: 5540 }
    }
}

pub struct MatterBridgeNode<C> {
    bridge: Arc<RelayBridge<C>>,
    config: MatterNodeConfig,
}

impl<C> MatterBridgeNode<C>
where
    C: BleBridgeClient + 'static,
{
    pub fn new(bridge: Arc<RelayBridge<C>>, config: MatterNodeConfig) -> Self {
        Self { bridge, config }
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub async fn run(&self) -> Result<()> {
        #[cfg(feature = "matter")]
        {
            return self.run_with_rs_matter().await;
        }

        #[cfg(not(feature = "matter"))]
        {
            let _ = &self.bridge;
            bail!(
                "hub was built without the `matter` feature; BLE bridge is active but rs-matter integration is disabled"
            );
        }
    }

    #[cfg(feature = "matter")]
    async fn run_with_rs_matter(&self) -> Result<()> {
        let initial = self.bridge.status().await;
        let mut kv_buf = [0_u8; 4096];
        let mut kv = DirKvBlobStore::new_default();

        let mut matter = Matter::new(
            &BASIC_INFO,
            TEST_DEV_COMM.clone(),
            &TEST_DEV_ATT,
            self.port(),
        );
        matter.load_persist(&mut kv, &mut kv_buf).await?;

        let buffers = PooledBuffers::<10, IMBuffer>::new(0);
        let subscriptions: Subscriptions = Subscriptions::new();
        let events = NoEvents::new();

        let crypto = default_crypto(rand::thread_rng(), DAC_PRIVKEY);
        let mut rand = crypto.rand()?;

        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let hook = BleBackedOnOff::new(self.bridge.subscribe(), command_tx, initial);
        let on_off_handler = OnOffHandler::new_standalone(Dataver::new_rand(&mut rand), 1, hook);

        let bridge = self.bridge.clone();
        tokio::spawn(async move {
            while let Some(next) = command_rx.recv().await {
                if let Err(error) = bridge.set_state(next).await {
                    error!("failed to apply Matter-driven BLE state change: {error}");
                }
            }
        });

        let dm = DataModel::new(
            &matter,
            &crypto,
            &buffers,
            &subscriptions,
            &events,
            dm_handler(rand, &on_off_handler),
            SharedKvBlobStore::new(kv, kv_buf.as_mut_slice()),
            SharedNetworks::new(EthNetwork::new_default()),
        );

        let responder = DefaultResponder::new(&dm);
        let mut respond = pin!(responder.run::<4, 4>());
        let mut dm_job = pin!(dm.run());
        let socket = async_io::Async::<std::net::UdpSocket>::bind(MATTER_SOCKET_BIND_ADDR)?;

        let mut mdns_responder = ZeroconfMdnsResponder::new();
        let mut mdns = pin!(mdns_responder.run(&matter));
        let mut transport = pin!(matter.run(&crypto, &socket, &socket, &socket));

        if !matter.is_commissioned() {
            matter.print_standard_qr_text(DiscoveryCapabilities::IP)?;
            matter.print_standard_qr_code(QrTextType::Unicode, DiscoveryCapabilities::IP)?;
            matter.open_basic_comm_window(MAX_COMM_WINDOW_TIMEOUT_SECS, &crypto, &())?;
            info!("Matter commissioning window opened on port {}", self.port());
        }

        select4(&mut transport, &mut mdns, &mut respond, &mut dm_job)
            .coalesce()
            .await
            .map_err(Into::into)
    }
}

#[cfg(feature = "matter")]
const NODE: Node<'static> = Node {
    endpoints: &[
        root_endpoint!(eth),
        Endpoint::new(
            1,
            devices!(DEV_TYPE_ON_OFF_LIGHT),
            clusters!(
                desc::DescHandler::CLUSTER,
                groups::GroupsHandler::CLUSTER,
                BleBackedOnOff::CLUSTER
            ),
        ),
    ],
};

#[cfg(feature = "matter")]
fn dm_handler<'a, OH: OnOffHooks, LH: LevelControlHooks>(
    mut rand: impl RngCore + Copy,
    on_off: &'a on_off::OnOffHandler<'a, OH, LH>,
) -> impl DataModelHandler + 'a {
    (
        NODE,
        endpoints::EthSysHandlerBuilder::new()
            .netif_diag(&SysNetifs)
            .build(rand)
            .chain(
                EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                Async(desc::DescHandler::new(Dataver::new_rand(&mut rand)).adapt()),
            )
            .chain(
                EpClMatcher::new(Some(1), Some(groups::GroupsHandler::CLUSTER.id)),
                Async(groups::GroupsHandler::new(Dataver::new_rand(&mut rand)).adapt()),
            )
            .chain(
                EpClMatcher::new(Some(1), Some(BleBackedOnOff::CLUSTER.id)),
                on_off::HandlerAsyncAdaptor(on_off),
            ),
    )
}

#[cfg(feature = "matter")]
struct BleBackedOnOff {
    current_state: AtomicBool,
    start_up_on_off: Mutex<Option<StartUpOnOffEnum>>,
    bridge_rx: Mutex<Option<watch::Receiver<BridgeStatus>>>,
    command_tx: mpsc::UnboundedSender<switcher_protocol::RelayState>,
}

#[cfg(feature = "matter")]
impl BleBackedOnOff {
    fn new(
        bridge_rx: watch::Receiver<BridgeStatus>,
        command_tx: mpsc::UnboundedSender<switcher_protocol::RelayState>,
        initial: BridgeStatus,
    ) -> Self {
        Self {
            current_state: AtomicBool::new(initial.relay_state.as_bool()),
            start_up_on_off: Mutex::new(None),
            bridge_rx: Mutex::new(Some(bridge_rx)),
            command_tx,
        }
    }
}

#[cfg(feature = "matter")]
impl OnOffHooks for BleBackedOnOff {
    const CLUSTER: rs_matter::dm::Cluster<'static> = on_off_cluster::FULL_CLUSTER
        .with_revision(6)
        .with_features(on_off_cluster::Feature::LIGHTING.bits())
        .with_attrs(with!(
            required;
            on_off_cluster::AttributeId::OnOff
                | on_off_cluster::AttributeId::GlobalSceneControl
                | on_off_cluster::AttributeId::OnTime
                | on_off_cluster::AttributeId::OffWaitTime
                | on_off_cluster::AttributeId::StartUpOnOff
        ))
        .with_cmds(with!(
            on_off_cluster::CommandId::Off
                | on_off_cluster::CommandId::On
                | on_off_cluster::CommandId::Toggle
                | on_off_cluster::CommandId::OffWithEffect
                | on_off_cluster::CommandId::OnWithRecallGlobalScene
                | on_off_cluster::CommandId::OnWithTimedOff
        ));

    fn on_off(&self) -> bool {
        self.current_state.load(Ordering::SeqCst)
    }

    fn set_on_off(&self, on: bool) {
        self.current_state.store(on, Ordering::SeqCst);
        let _ = self
            .command_tx
            .send(switcher_protocol::RelayState::from_bool(on));
    }

    fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        self.start_up_on_off
            .lock()
            .unwrap()
            .map(Nullable::some)
            .unwrap_or_else(Nullable::none)
    }

    fn set_start_up_on_off(&self, value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        *self.start_up_on_off.lock().unwrap() = value.into_option();
        Ok(())
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {}

    async fn run<F: Fn(OutOfBandMessage)>(&self, notify: F) {
        let mut rx = self
            .bridge_rx
            .lock()
            .unwrap()
            .take()
            .expect("OnOffHooks::run must only be called once");

        loop {
            if rx.changed().await.is_err() {
                core::future::pending::<()>().await;
            }

            let state = rx.borrow().relay_state.as_bool();
            self.current_state.store(state, Ordering::SeqCst);
            notify(OutOfBandMessage::Update);
        }
    }
}
