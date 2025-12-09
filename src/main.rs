#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_time::{Duration, Timer};
use embedded_hal::digital::v2::{OutputPin, PinState};
use panic_probe as _;

const NUM_ELECTRODES: usize = 4;
const NUM_PROGRAMS: usize = 1;

const H: bool = true;
const L: bool = false;

static DELAY: u64 = 47;

static PROGRAMS: [&[[bool; NUM_ELECTRODES]]; NUM_PROGRAMS] = [
    &[
        [H, L, H, L],
        [L, H, L, H],
    ],
];

static mut SELECTED_PROGRAM: usize = 0;

#[embassy_executor::task]
async fn button_task(mut button: ExtiInput<'static>) {
    loop {
        button.wait_for_falling_edge().await;

        unsafe {
            SELECTED_PROGRAM = (SELECTED_PROGRAM + 1) % NUM_PROGRAMS;
        }

        // Debounce
        Timer::after(Duration::from_millis(200)).await;
    }
}

#[embassy_executor::task]
async fn stimulator_task(mut electrodes: [Output<'static>; NUM_ELECTRODES]) {
    loop {
        let program = unsafe { PROGRAMS[SELECTED_PROGRAM] };
        for stage in program.iter() {
            for (el, st) in electrodes.iter_mut().zip(stage.iter()) {
                el.set_state(if *st { PinState::High } else { PinState::Low }).unwrap();
            }
            Timer::after(Duration::from_millis(DELAY)).await;
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    let button = ExtiInput::new(p.PA1, p.EXTI1, Pull::Up);

    let el1 = Output::new(p.PB12, Level::High, Speed::Medium);
    let el2 = Output::new(p.PB13, Level::High, Speed::Medium);
    let el3 = Output::new(p.PB15, Level::High, Speed::Medium);
    let el4 = Output::new(p.PA8, Level::High, Speed::Medium);

    spawner.spawn(button_task(button)).unwrap();
    spawner.spawn(stimulator_task([el1, el2, el3, el4])).unwrap();
}