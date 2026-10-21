//! The whole firmware, unmodified.
//!
//! Joins the swarm and hosts a WASM cell.

#![no_std]
#![no_main]

use esp_firmware::{Board, Network, embassy_executor, esp_hal::peripherals::Peripherals};
use embassy_executor::Spawner;

#[esp_firmware::main]
async fn setup(
    board: &mut Board,
    net: Network,
    peripherals: Peripherals,
    spawner: Spawner,
) {
    loop {
        esp_firmware::log::info!("hello");
        embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;
    }
}


#[embassy_executor::task(embassy_executor = esp_firmware::embassy_executor)]
async fn my_task() {}

