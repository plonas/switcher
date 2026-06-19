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
use anyhow::{Result as AnyResult, bail};
#[cfg(not(feature = "matter"))]
use anyhow::{Result as AnyResult, bail};
#[cfg(feature = "matter")]
use embassy_futures::select::{select, select3, select4};
#[cfg(feature = "matter")]
use log::{error, info};
#[cfg(feature = "matter")]
use rs_matter::crypto::{Crypto, RngCore, default_crypto};
#[cfg(feature = "matter")]
use rs_matter::dm::IMBuffer;
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
use rs_matter::dm::devices::{DEV_TYPE_AGGREGATOR, DEV_TYPE_BRIDGED_NODE, DEV_TYPE_ON_OFF_LIGHT};
#[cfg(feature = "matter")]
use rs_matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM};
#[cfg(feature = "matter")]
use rs_matter::dm::endpoints::{self, EthSysHandler};
#[cfg(feature = "matter")]
use rs_matter::dm::events::NoEvents;
#[cfg(feature = "matter")]
use rs_matter::dm::networks::SysNetifs;
#[cfg(feature = "matter")]
use rs_matter::dm::networks::eth::EthNetwork;
#[cfg(feature = "matter")]
use rs_matter::dm::subscriptions::Subscriptions;
#[cfg(feature = "matter")]
use rs_matter::dm::{Async, AsyncHandler, DataModel, Dataver, Endpoint, HandlerContext, InvokeContext, InvokeReply, MatchContext, Node, ReadContext, ReadReply, WriteContext};
#[cfg(feature = "matter")]
use rs_matter::dm::{
    clusters::decl::bridged_device_basic_information::{self, KeepActiveRequest},
    Cluster, EndptId,
};
#[cfg(feature = "matter")]
use rs_matter::error::{Error, ErrorCode};
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
use rs_matter::tlv::{Nullable, TLVBuilderParent, Utf8StrBuilder};
#[cfg(feature = "matter")]
use rs_matter::transport::MATTER_SOCKET_BIND_ADDR;
#[cfg(feature = "matter")]
use rs_matter::transport::network::mdns::zeroconf::ZeroconfMdnsResponder;
#[cfg(feature = "matter")]
use rs_matter::utils::select::Coalesce;
#[cfg(feature = "matter")]
use rs_matter::utils::storage::pooled::PooledBuffers;
#[cfg(feature = "matter")]
use rs_matter::{Matter, devices, root_endpoint, with};
#[cfg(feature = "matter")]
use tokio::sync::{mpsc, watch};

#[cfg(not(feature = "matter"))]
use crate::{ble::BleBridgeClient, fleet::DongleFleet};
#[cfg(feature = "matter")]
use crate::{
    ble::BleBridgeClient,
    bridge::BridgeStatus,
    fleet::{DongleFleet, FleetSlot},
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

#[cfg(feature = "matter")]
const MAX_BRIDGED_DONGLES: usize = 4;

#[cfg(feature = "matter")]
const BRIDGED_ON_OFF_CLUSTER: Cluster<'static> = on_off_cluster::FULL_CLUSTER
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

#[cfg(feature = "matter")]
const BRIDGED_INFO_CLUSTER: Cluster<'static> =
    bridged_device_basic_information::FULL_CLUSTER
        .with_features(0)
        .with_attrs(with!(required))
        .with_cmds(with!());

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
    fleet: Arc<DongleFleet<C>>,
    config: MatterNodeConfig,
}

impl<C> MatterBridgeNode<C>
where
    C: BleBridgeClient + 'static,
{
    pub fn new(fleet: Arc<DongleFleet<C>>, config: MatterNodeConfig) -> Self {
        Self { fleet, config }
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub async fn run(&self) -> AnyResult<()> {
        #[cfg(feature = "matter")]
        {
            return self.run_with_rs_matter().await;
        }

        #[cfg(not(feature = "matter"))]
        {
            let _ = &self.fleet;
            bail!(
                "hub was built without the `matter` feature; BLE bridge is active but rs-matter integration is disabled"
            );
        }
    }

    #[cfg(feature = "matter")]
    async fn run_with_rs_matter(&self) -> AnyResult<()> {
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
        let rand = crypto.rand()?;

        let active_slots = self.collect_active_slots().await?;
        let runtime = MatterRuntime::new(active_slots, rand);

        let dm = DataModel::new(
            &matter,
            &crypto,
            &buffers,
            &subscriptions,
            &events,
            (runtime.node, runtime.handler),
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

    #[cfg(feature = "matter")]
    async fn collect_active_slots(&self) -> AnyResult<Vec<ActiveMatterSlot<C>>> {
        let enabled: Vec<_> = self.fleet.enabled_slots().collect();
        if enabled.len() > MAX_BRIDGED_DONGLES {
            bail!(
                "at most {MAX_BRIDGED_DONGLES} enabled dongles are supported in this release, got {}",
                enabled.len()
            );
        }

        let mut slots = Vec::with_capacity(enabled.len());
        for slot in enabled {
            slots.push(ActiveMatterSlot::from_fleet_slot(slot).await);
        }

        Ok(slots)
    }
}

#[cfg(feature = "matter")]
struct MatterRuntime {
    node: Node<'static>,
    handler: MatterDispatchHandler,
}

#[cfg(feature = "matter")]
impl MatterRuntime {
    fn new<C>(active_slots: Vec<ActiveMatterSlot<C>>, mut rand: impl RngCore + Copy) -> Self
    where
        C: BleBridgeClient + 'static,
    {
        let endpoint_ids: Vec<EndptId> = active_slots.iter().map(|slot| slot.endpoint_id).collect();
        let node = build_node(&endpoint_ids);
        let root = endpoints::EthSysHandlerBuilder::new()
            .netif_diag(&SysNetifs)
            .build(rand);
        let aggregator_desc = Async(desc::DescHandler::new_aggregator(Dataver::new_rand(&mut rand)).adapt());
        let slots = MatterSlotHandler::new(active_slots, &mut rand);

        Self {
            node,
            handler: MatterDispatchHandler {
                root,
                aggregator_desc,
                slots,
            },
        }
    }
}

#[cfg(feature = "matter")]
struct ActiveMatterSlot<C> {
    endpoint_id: EndptId,
    device_id: String,
    bridge: Arc<crate::bridge::RelayBridge<C>>,
    initial: BridgeStatus,
}

#[cfg(feature = "matter")]
impl<C> ActiveMatterSlot<C>
where
    C: BleBridgeClient + 'static,
{
    async fn from_fleet_slot(slot: &FleetSlot<C>) -> Self {
        Self {
            endpoint_id: slot.registration.endpoint_id,
            device_id: slot.registration.device_id.clone(),
            bridge: slot.bridge.clone(),
            initial: slot.bridge.status().await,
        }
    }
}

#[cfg(feature = "matter")]
const ROOT_ENDPOINT: Endpoint<'static> = root_endpoint!(eth);
#[cfg(feature = "matter")]
const AGGREGATOR_ENDPOINT: Endpoint<'static> = Endpoint::new(
    1,
    devices!(DEV_TYPE_AGGREGATOR),
    &[desc::DescHandler::CLUSTER],
);
#[cfg(feature = "matter")]
const BRIDGED_DEVICE_TYPES: &[rs_matter::dm::DeviceType] =
    devices!(DEV_TYPE_ON_OFF_LIGHT, DEV_TYPE_BRIDGED_NODE);
#[cfg(feature = "matter")]
const BRIDGED_CLUSTERS: &[Cluster<'static>] = &[
    desc::DescHandler::CLUSTER,
    groups::GroupsHandler::CLUSTER,
    BRIDGED_INFO_CLUSTER,
    BRIDGED_ON_OFF_CLUSTER,
];

#[cfg(feature = "matter")]
fn build_node(endpoint_ids: &[EndptId]) -> Node<'static> {
    let mut endpoints = Vec::with_capacity(endpoint_ids.len() + 2);
    endpoints.push(ROOT_ENDPOINT);
    endpoints.push(AGGREGATOR_ENDPOINT);

    for endpoint_id in endpoint_ids {
        endpoints.push(Endpoint::new(
            *endpoint_id,
            BRIDGED_DEVICE_TYPES,
            BRIDGED_CLUSTERS,
        ));
    }

    let leaked = Box::leak(endpoints.into_boxed_slice());
    Node::new(leaked)
}

#[cfg(feature = "matter")]
type OnOffSlotHandler =
    on_off::HandlerAsyncAdaptor<OnOffHandler<'static, BridgeOnOffHooks, on_off::NoLevelControl>>;
#[cfg(feature = "matter")]
type DescSlotHandler = Async<desc::HandlerAdaptor<desc::DescHandler<'static>>>;
#[cfg(feature = "matter")]
type GroupsSlotHandler = Async<groups::HandlerAdaptor<groups::GroupsHandler>>;
#[cfg(feature = "matter")]
type BridgedInfoSlotHandler =
    Async<bridged_device_basic_information::HandlerAdaptor<BridgedInfoHandler>>;

#[cfg(feature = "matter")]
struct MatterDispatchHandler {
    root: EthSysHandler<'static>,
    aggregator_desc: DescSlotHandler,
    slots: MatterSlotHandler,
}

#[cfg(feature = "matter")]
impl AsyncHandler for MatterDispatchHandler {
    fn read_awaits(&self, ctx: impl ReadContext) -> bool {
        if ctx.attr().endpoint_id == 0 {
            self.root.read_awaits(ctx)
        } else if ctx.attr().endpoint_id == 1 && ctx.attr().cluster_id == desc::DescHandler::CLUSTER.id {
            self.aggregator_desc.read_awaits(ctx)
        } else {
            self.slots.read_awaits(ctx)
        }
    }

    fn write_awaits(&self, ctx: impl WriteContext) -> bool {
        if ctx.attr().endpoint_id == 0 {
            self.root.write_awaits(ctx)
        } else if ctx.attr().endpoint_id == 1 && ctx.attr().cluster_id == desc::DescHandler::CLUSTER.id {
            self.aggregator_desc.write_awaits(ctx)
        } else {
            self.slots.write_awaits(ctx)
        }
    }

    fn invoke_awaits(&self, ctx: impl InvokeContext) -> bool {
        if ctx.cmd().endpoint_id == 0 {
            self.root.invoke_awaits(ctx)
        } else if ctx.cmd().endpoint_id == 1 && ctx.cmd().cluster_id == desc::DescHandler::CLUSTER.id {
            self.aggregator_desc.invoke_awaits(ctx)
        } else {
            self.slots.invoke_awaits(ctx)
        }
    }

    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        if ctx.attr().endpoint_id == 0 {
            self.root.read(ctx, reply).await
        } else if ctx.attr().endpoint_id == 1 && ctx.attr().cluster_id == desc::DescHandler::CLUSTER.id {
            self.aggregator_desc.read(ctx, reply).await
        } else {
            self.slots.read(ctx, reply).await
        }
    }

    async fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        if ctx.attr().endpoint_id == 0 {
            self.root.write(ctx).await
        } else if ctx.attr().endpoint_id == 1 && ctx.attr().cluster_id == desc::DescHandler::CLUSTER.id {
            self.aggregator_desc.write(ctx).await
        } else {
            self.slots.write(ctx).await
        }
    }

    async fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        if ctx.cmd().endpoint_id == 0 {
            self.root.invoke(ctx, reply).await
        } else if ctx.cmd().endpoint_id == 1 && ctx.cmd().cluster_id == desc::DescHandler::CLUSTER.id {
            self.aggregator_desc.invoke(ctx, reply).await
        } else {
            self.slots.invoke(ctx, reply).await
        }
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        match ctx.endpt() {
            Some(0) => self.root.bump_dataver(ctx),
            Some(1) if ctx.cluster() == Some(desc::DescHandler::CLUSTER.id) => {
                self.aggregator_desc.bump_dataver(ctx)
            }
            _ => self.slots.bump_dataver(ctx),
        }
    }

    async fn run(&self, ctx: impl HandlerContext) -> Result<(), Error> {
        let mut root = pin!(self.root.run(&ctx));
        let mut aggregator = pin!(self.aggregator_desc.run(&ctx));
        let mut slots = pin!(self.slots.run(&ctx));

        select3(&mut root, &mut aggregator, &mut slots)
            .coalesce()
            .await
    }
}

#[cfg(feature = "matter")]
struct MatterSlotHandler {
    slots: Vec<MatterBridgeSlot>,
}

#[cfg(feature = "matter")]
struct MatterBridgeSlot {
    endpoint_id: EndptId,
    desc: DescSlotHandler,
    groups: GroupsSlotHandler,
    on_off: OnOffSlotHandler,
    bridged: BridgedInfoSlotHandler,
}

#[cfg(feature = "matter")]
impl MatterSlotHandler {
    fn new<C>(active_slots: Vec<ActiveMatterSlot<C>>, rand: &mut (impl RngCore + Copy)) -> Self
    where
        C: BleBridgeClient + 'static,
    {
        let mut slots = Vec::with_capacity(active_slots.len());

        for active in active_slots {
            let mirror = Arc::new(BridgeMirror::new(
                active.bridge.subscribe(),
                active.initial,
                active.bridge.clone(),
            ));
            let hooks = BridgeOnOffHooks(mirror.clone());
            let on_off = OnOffHandler::new_standalone(
                Dataver::new_rand(rand),
                active.endpoint_id,
                hooks,
            )
            .adapt();

            slots.push(MatterBridgeSlot {
                endpoint_id: active.endpoint_id,
                desc: Async(desc::DescHandler::new(Dataver::new_rand(rand)).adapt()),
                groups: Async(groups::GroupsHandler::new(Dataver::new_rand(rand)).adapt()),
                on_off,
                bridged: Async(
                    BridgedInfoHandler::new(
                        Dataver::new_rand(rand),
                        active.device_id,
                        mirror,
                    )
                    .adapt(),
                ),
            });
        }

        Self { slots }
    }

    fn slot_for(&self, endpoint_id: EndptId) -> Option<&MatterBridgeSlot> {
        self.slots.iter().find(|slot| slot.endpoint_id == endpoint_id)
    }
}

#[cfg(feature = "matter")]
impl AsyncHandler for MatterSlotHandler {
    fn read_awaits(&self, ctx: impl ReadContext) -> bool {
        let Some(slot) = self.slot_for(ctx.attr().endpoint_id) else {
            return false;
        };

        match ctx.attr().cluster_id {
            id if id == desc::DescHandler::CLUSTER.id => slot.desc.read_awaits(ctx),
            id if id == groups::GroupsHandler::CLUSTER.id => slot.groups.read_awaits(ctx),
            id if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.read_awaits(ctx),
            id if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.read_awaits(ctx),
            _ => false,
        }
    }

    fn write_awaits(&self, ctx: impl WriteContext) -> bool {
        let Some(slot) = self.slot_for(ctx.attr().endpoint_id) else {
            return false;
        };

        match ctx.attr().cluster_id {
            id if id == desc::DescHandler::CLUSTER.id => slot.desc.write_awaits(ctx),
            id if id == groups::GroupsHandler::CLUSTER.id => slot.groups.write_awaits(ctx),
            id if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.write_awaits(ctx),
            id if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.write_awaits(ctx),
            _ => false,
        }
    }

    fn invoke_awaits(&self, ctx: impl InvokeContext) -> bool {
        let Some(slot) = self.slot_for(ctx.cmd().endpoint_id) else {
            return false;
        };

        match ctx.cmd().cluster_id {
            id if id == desc::DescHandler::CLUSTER.id => slot.desc.invoke_awaits(ctx),
            id if id == groups::GroupsHandler::CLUSTER.id => slot.groups.invoke_awaits(ctx),
            id if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.invoke_awaits(ctx),
            id if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.invoke_awaits(ctx),
            _ => false,
        }
    }

    async fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let Some(slot) = self.slot_for(ctx.attr().endpoint_id) else {
            return Err(ErrorCode::EndpointNotFound.into());
        };

        match ctx.attr().cluster_id {
            id if id == desc::DescHandler::CLUSTER.id => slot.desc.read(ctx, reply).await,
            id if id == groups::GroupsHandler::CLUSTER.id => slot.groups.read(ctx, reply).await,
            id if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.read(ctx, reply).await,
            id if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.read(ctx, reply).await,
            _ => Err(ErrorCode::ClusterNotFound.into()),
        }
    }

    async fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let Some(slot) = self.slot_for(ctx.attr().endpoint_id) else {
            return Err(ErrorCode::EndpointNotFound.into());
        };

        match ctx.attr().cluster_id {
            id if id == desc::DescHandler::CLUSTER.id => slot.desc.write(ctx).await,
            id if id == groups::GroupsHandler::CLUSTER.id => slot.groups.write(ctx).await,
            id if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.write(ctx).await,
            id if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.write(ctx).await,
            _ => Err(ErrorCode::ClusterNotFound.into()),
        }
    }

    async fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        let Some(slot) = self.slot_for(ctx.cmd().endpoint_id) else {
            return Err(ErrorCode::EndpointNotFound.into());
        };

        match ctx.cmd().cluster_id {
            id if id == desc::DescHandler::CLUSTER.id => slot.desc.invoke(ctx, reply).await,
            id if id == groups::GroupsHandler::CLUSTER.id => slot.groups.invoke(ctx, reply).await,
            id if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.invoke(ctx, reply).await,
            id if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.invoke(ctx, reply).await,
            _ => Err(ErrorCode::ClusterNotFound.into()),
        }
    }

    fn bump_dataver(&self, ctx: impl MatchContext) {
        let Some(endpoint_id) = ctx.endpt() else {
            return;
        };
        let Some(slot) = self.slot_for(endpoint_id) else {
            return;
        };

        match ctx.cluster() {
            Some(id) if id == desc::DescHandler::CLUSTER.id => slot.desc.bump_dataver(ctx),
            Some(id) if id == groups::GroupsHandler::CLUSTER.id => slot.groups.bump_dataver(ctx),
            Some(id) if id == BRIDGED_INFO_CLUSTER.id => slot.bridged.bump_dataver(ctx),
            Some(id) if id == BRIDGED_ON_OFF_CLUSTER.id => slot.on_off.bump_dataver(ctx),
            _ => {}
        }
    }

    async fn run(&self, ctx: impl HandlerContext) -> Result<(), Error> {
        match self.slots.as_slice() {
            [] => core::future::pending::<Result<(), Error>>().await,
            [slot] => slot.on_off.run(ctx).await,
            [first, second] => {
                let mut first = pin!(first.on_off.run(&ctx));
                let mut second = pin!(second.on_off.run(&ctx));
                select(&mut first, &mut second).coalesce().await
            }
            [first, second, third] => {
                let mut first = pin!(first.on_off.run(&ctx));
                let mut second = pin!(second.on_off.run(&ctx));
                let mut third = pin!(third.on_off.run(&ctx));
                select3(&mut first, &mut second, &mut third)
                    .coalesce()
                    .await
            }
            [first, second, third, fourth] => {
                let mut first = pin!(first.on_off.run(&ctx));
                let mut second = pin!(second.on_off.run(&ctx));
                let mut third = pin!(third.on_off.run(&ctx));
                let mut fourth = pin!(fourth.on_off.run(&ctx));
                select4(&mut first, &mut second, &mut third, &mut fourth)
                    .coalesce()
                    .await
            }
            _ => unreachable!("slot count is capped by MAX_BRIDGED_DONGLES"),
        }
    }
}

#[cfg(feature = "matter")]
#[derive(Debug)]
struct BridgeMirror {
    current_state: AtomicBool,
    connected: AtomicBool,
    start_up_on_off: Mutex<Option<StartUpOnOffEnum>>,
    bridge_rx: Mutex<Option<watch::Receiver<BridgeStatus>>>,
    command_tx: mpsc::UnboundedSender<switcher_protocol::RelayState>,
}

#[cfg(feature = "matter")]
#[derive(Clone, Debug)]
struct BridgeOnOffHooks(Arc<BridgeMirror>);

#[cfg(feature = "matter")]
impl BridgeMirror {
    fn new<C>(
        bridge_rx: watch::Receiver<BridgeStatus>,
        initial: BridgeStatus,
        bridge: Arc<crate::bridge::RelayBridge<C>>,
    ) -> Self
    where
        C: BleBridgeClient + 'static,
    {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(next) = command_rx.recv().await {
                if let Err(error) = bridge.set_state(next).await {
                    error!("failed to apply Matter-driven BLE state change: {error}");
                }
            }
        });

        Self {
            current_state: AtomicBool::new(initial.relay_state.as_bool()),
            connected: AtomicBool::new(initial.connected),
            start_up_on_off: Mutex::new(None),
            bridge_rx: Mutex::new(Some(bridge_rx)),
            command_tx,
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

#[cfg(feature = "matter")]
impl OnOffHooks for BridgeOnOffHooks {
    const CLUSTER: Cluster<'static> = BRIDGED_ON_OFF_CLUSTER;

    fn on_off(&self) -> bool {
        self.0.current_state.load(Ordering::SeqCst)
    }

    fn set_on_off(&self, on: bool) {
        self.0.current_state.store(on, Ordering::SeqCst);
        let _ = self
            .0
            .command_tx
            .send(switcher_protocol::RelayState::from_bool(on));
    }

    fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        self.0
            .start_up_on_off
            .lock()
            .unwrap()
            .map(Nullable::some)
            .unwrap_or_else(Nullable::none)
    }

    fn set_start_up_on_off(&self, value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        *self.0.start_up_on_off.lock().unwrap() = value.into_option();
        Ok(())
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {}

    async fn run<F: Fn(OutOfBandMessage)>(&self, notify: F) {
        let mut rx = self
            .0
            .bridge_rx
            .lock()
            .unwrap()
            .take()
            .expect("OnOffHooks::run must only be called once");

        loop {
            if rx.changed().await.is_err() {
                core::future::pending::<()>().await;
            }

            let status = rx.borrow().clone();
            self.0
                .current_state
                .store(status.relay_state.as_bool(), Ordering::SeqCst);
            self.0.connected.store(status.connected, Ordering::SeqCst);
            notify(OutOfBandMessage::Update);
        }
    }
}

#[cfg(feature = "matter")]
#[derive(Clone, Debug)]
struct BridgedInfoHandler {
    dataver: Dataver,
    unique_id: String,
    mirror: Arc<BridgeMirror>,
}

#[cfg(feature = "matter")]
impl BridgedInfoHandler {
    fn new(dataver: Dataver, unique_id: String, mirror: Arc<BridgeMirror>) -> Self {
        Self {
            dataver,
            unique_id,
            mirror,
        }
    }

    pub const fn adapt(self) -> bridged_device_basic_information::HandlerAdaptor<Self> {
        bridged_device_basic_information::HandlerAdaptor(self)
    }
}

#[cfg(feature = "matter")]
impl bridged_device_basic_information::ClusterHandler for BridgedInfoHandler {
    const CLUSTER: Cluster<'static> = BRIDGED_INFO_CLUSTER;

    fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    fn dataver_changed(&self) {
        self.dataver.changed();
    }

    fn reachable(&self, _ctx: impl ReadContext) -> Result<bool, Error> {
        Ok(self.mirror.is_connected())
    }

    fn unique_id<P: TLVBuilderParent>(
        &self,
        _ctx: impl ReadContext,
        builder: Utf8StrBuilder<P>,
    ) -> Result<P, Error> {
        builder.set(self.unique_id.as_str())
    }

    fn handle_keep_active(
        &self,
        _ctx: impl InvokeContext,
        _request: KeepActiveRequest<'_>,
    ) -> Result<(), Error> {
        Ok(())
    }
}
