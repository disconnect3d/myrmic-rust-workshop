//! Joins the swarm as a native cell and logs debounced GPIO9 button presses.

#![no_std]
#![no_main]

extern crate alloc;

mod status_led;

use esp_firmware::{
    Board, Cell, Network, Sri, embassy_executor,
    embassy_executor::Spawner,
    embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel},
    embassy_time::{Instant, Timer},
    esp_hal::{
        gpio::{Input, InputConfig, Pull},
        peripherals::Peripherals,
    },
};

const DEBOUNCE_MS: u64 = 30;

// Keep detecting and logging clicks while the cell waits for the network.
static FIRE_REQUESTS: Channel<CriticalSectionRawMutex, (), 8> = Channel::new();

#[derive(serde::Serialize)]
enum Action {
    Fire,
}

#[derive(serde::Serialize)]
struct Command<'a> {
    action: Action,
    name: &'a str,
}

#[esp_firmware::main]
async fn setup(_board: &mut Board, net: Network, peripherals: Peripherals, spawner: Spawner) {
    status_led::StatusLed::new(peripherals.LEDC, peripherals.GPIO7, 16).spawn(spawner);

    let button = Input::new(peripherals.GPIO9, InputConfig::default().with_pull(Pull::Up));
    spawner.spawn(button_task(button).unwrap());

    let cell = match net.register("disco-srn", &[]) {
        Ok(cell) => cell,
        Err(err) => {
            esp_firmware::log::error!("unable to register cell: {err:?}");
            return;
        }
    };

    spawner.spawn(cell_task(cell).unwrap());
}

#[embassy_executor::task(embassy_executor = esp_firmware::embassy_executor)]
async fn cell_task(mut cell: Cell) {
    let router = Sri::from_target("router").unwrap();
    let fire = Command {
        action: Action::Fire,
        name: "moon",
    };
    loop {
        FIRE_REQUESTS.receive().await;
        cell.online().await;

        let mut tx = cell.open().await;
        let res = tx.send(router, "tank", &fire).await;
        if let Err(err) = res {
            esp_firmware::log::error!("unable to send fire command: {err:?}");
            let _ = tx.rollback().await;
            continue;
        }

        if let Err(err) = tx.commit().await {
            esp_firmware::log::error!("unable to commit fire command: {err:?}");
        }
    }
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

        if FIRE_REQUESTS.try_send(()).is_err() {
            esp_firmware::log::warn!("fire queue full; click was not queued");
        }

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
