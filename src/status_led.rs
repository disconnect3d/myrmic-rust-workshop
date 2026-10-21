//! A single-colour LED that reports what the node has reached on the network.
//!
//! | LED | Meaning |
//! | --- | ------- |
//! | Off | No WiFi association |
//! | Fast flash | WiFi up, but no myrmic session — the swarm is out of reach |
//! | Slow flash | Session up, but this node is not in the exec registry yet |
//! | Solid | Registered: the node is in `m network` and can be deployed to |
//!
//! The states come from [`esp_firmware::status`]. The LED is PWM-dimmed
//! through LEDC, since one driven straight off a pin is usually glaring.
//!
//! A [`StatusLed`] is a future that never finishes: `.await` it inside a task
//! of your own, or hand it to [`StatusLed::spawn`] for a task of its own.
//!
//! ```ignore
//! StatusLed::new(peripherals.LEDC, peripherals.GPIO7, 16).spawn(spawner);
//! ```

use alloc::boxed::Box;
use core::convert::Infallible;
use core::future::IntoFuture;
use core::pin::Pin;

use embassy_futures::select::{Either, select};
use esp_firmware::embassy_time::{Duration, Timer};
use esp_firmware::esp_hal::gpio::DriveMode;
use esp_firmware::esp_hal::gpio::interconnect::PeripheralOutput;
use esp_firmware::esp_hal::ledc::channel::{self, ChannelHW, ChannelIFace};
use esp_firmware::esp_hal::ledc::timer::{self, TimerIFace};
use esp_firmware::esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
use esp_firmware::esp_hal::peripherals::LEDC;
use esp_firmware::esp_hal::time::Rate;
use esp_firmware::static_cell::StaticCell;
use esp_firmware::embassy_executor::Spawner;
use esp_firmware::{NodeStatus, embassy_executor, log, status, status_watch};

/// Half-period of the flash with WiFi up but no session.
const FAST_BLINK: Duration = Duration::from_millis(150);
/// Half-period of the flash while waiting to be registered.
const SLOW_BLINK: Duration = Duration::from_millis(600);

/// The status LED: an LEDC channel and the duty it is lit at.
pub struct StatusLed {
    channel: channel::Channel<'static, LowSpeed>,
    brightness: u32,
}

impl StatusLed {
    /// Drives `pin` (lit when high) from LEDC timer 0 / channel 0, lit at
    /// `brightness` out of 1024 — 10-bit duty gives steps finer than the 1%
    /// `set_duty` allows. Takes the whole LEDC, so call it at most once.
    pub fn new(ledc: LEDC<'static>, pin: impl PeripheralOutput<'static>, brightness: u32) -> Self {
        let mut ledc = Ledc::new(ledc);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

        // The channel borrows its timer for as long as it runs.
        static TIMER: StaticCell<timer::Timer<'static, LowSpeed>> = StaticCell::new();
        let timer = TIMER.init(ledc.timer(timer::Number::Timer0));
        timer
            .configure(timer::config::Config {
                duty: timer::config::Duty::Duty10Bit,
                clock_source: timer::LSClockSource::APBClk,
                frequency: Rate::from_khz(20),
            })
            .unwrap();

        let mut channel = ledc.channel(channel::Number::Channel0, pin);
        channel
            .configure(channel::config::Config {
                timer: &*timer,
                duty_pct: 0,
                drive_mode: DriveMode::PushPull,
            })
            .unwrap();

        let brightness = brightness.min(1024u32);

        Self { channel, brightness }
    }

    fn set(&self, on: bool) {
        self.channel.set_duty_hw(if on { self.brightness } else { 0 });
    }
}

/// Boxed because stable Rust cannot name an `async fn`'s future; it is one
/// allocation, made once when the LED starts.
impl IntoFuture for StatusLed {
    type Output = Infallible;
    type IntoFuture = Pin<Box<dyn Future<Output = Infallible>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(run(self))
    }
}

impl StatusLed {
    /// Runs the LED in a task of its own.
    pub fn spawn(self, spawner: Spawner) {
        spawner.spawn(task(self).unwrap());
    }
}

#[embassy_executor::task(embassy_executor = esp_firmware::embassy_executor)]
async fn task(led: StatusLed) {
    led.await;
}

#[embassy_executor::task(embassy_executor = esp_firmware::embassy_executor)]
async fn my_task() {
}

/// Paints the node's status on the LED. Holds one of the two status watchers
/// for as long as it runs.
async fn run(led: StatusLed) -> Infallible {
    let mut watch = status_watch().expect("both status watchers already taken");
    let mut lit = true;
    let mut said = None;

    loop {
        // A flashing state alternates on its blink timer; a steady one parks
        // until the status changes.
        let status = status();
        let (on, blink) = match status {
            NodeStatus::Down => (false, None),
            NodeStatus::Associated => (lit, Some(FAST_BLINK)),
            NodeStatus::Connected => (lit, Some(SLOW_BLINK)),
            NodeStatus::Registered => (true, None),
        };
        // One line per status, not per blink edge, so the log says what the
        // light is saying.
        if said != Some(status) {
            said = Some(status);
            log::info!("[status] {status:?}");
        }
        led.set(on);

        match select(watch.changed(), edge(blink)).await {
            Either::First(_status) => lit = true,
            Either::Second(()) => lit = !lit,
        }
    }
}

/// The next blink edge, or never for a steady state.
async fn edge(blink: Option<Duration>) {
    match blink {
        Some(half_period) => Timer::after(half_period).await,
        None => core::future::pending::<()>().await,
    }
}
