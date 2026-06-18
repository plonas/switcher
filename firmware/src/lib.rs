#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};

pub mod app;
pub mod ble;
pub mod board;
pub mod relay;
pub mod trouble;

pub use app::{FirmwareApp, FirmwareStatus};

static DEFMT_TICKS: AtomicU32 = AtomicU32::new(0);

defmt::timestamp!("{=u32:us}", { DEFMT_TICKS.fetch_add(1, Ordering::Relaxed) });
