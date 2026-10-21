//! Joins the swarm, hosts a WASM cell, and logs debounced GPIO9 button presses.

#![no_std]
#![no_main]

use esp_firmware::{
    Board, embassy_executor,
    embassy_executor::Spawner,
    embassy_time::{Instant, Timer},
    esp_hal::{
        gpio::{Input, InputConfig, Pull},
        peripherals::Peripherals,
    },
};

const DEBOUNCE_MS: u64 = 30;

#[esp_firmware::main]
async fn setup(_board: &mut Board, peripherals: Peripherals, spawner: Spawner) {
    let button = Input::new(peripherals.GPIO9, InputConfig::default().with_pull(Pull::Up));
    spawner.spawn(button_task(button).unwrap());
}

#[embassy_executor::task(embassy_executor = esp_firmware::embassy_executor)]
async fn button_task(mut button: Input<'static>) {
    loop {
        button.wait_for_low().await;
        Timer::after_millis(DEBOUNCE_MS).await;
        if button.is_high() {
            continue;
        }

        let now = Instant::now();
        esp_firmware::log::info!("[button] clicked @ {}", now.as_millis());

        // Require a stable release before accepting another press.
        loop {
            button.wait_for_high().await;
            Timer::after_millis(DEBOUNCE_MS).await;
            if button.is_high() {
                break;
            }
        }
    }
}
