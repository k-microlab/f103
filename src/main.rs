#![no_std]
#![no_main]

use defmt::Format;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_stm32::Config;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::time::Hertz;
use embassy_time::{Instant, Timer};
use smart_leds::RGB8;

mod led;
use crate::led::Led;

const MAX_BRIGHTNESS: u8 = 100;
const NUM_ELECTRODES: usize = 2;
const NUM_PROGRAMS: usize = 1;

const H: bool = true;
const L: bool = false;


const MODES: usize = 5;
static mut MODE: usize = 0;
static mut CHARGING: bool = false;
static mut STANDBY: bool = true;

static mut STATE: ChargingState = ChargingState::Unknown;

#[derive(Debug, Format, Copy, Clone, PartialEq, Eq)]
enum ChargingState {
    InProgress,
    Finished,
    BatteryProblem,
    Unknown,
}

static PROGRAMS: [&[[bool; NUM_ELECTRODES]]; NUM_PROGRAMS] = [
    &[
        [H, L/*, H, L*/],
        [L, H/*, L, H*/],
    ],
];

static mut SELECTED_PROGRAM: usize = 0;

#[embassy_executor::task]
async fn button_task(mut button: ExtiInput<'static>) {
    loop {
        button.wait_for_falling_edge().await;

        // Debounce
        Timer::after_millis(50).await;

        let mut was_pressed = false;
        let start = Instant::now();

        if button.is_low() {
            was_pressed = true;
            defmt::info!("Button pressed");
            unsafe {
            }
        }

        button.wait_for_rising_edge().await;

        // Debounce
        Timer::after_millis(50).await;

        if button.is_high() {
            defmt::info!("Button unpressed: {}", start.elapsed());
            unsafe {
                MODE = (MODE + 1) % MODES;
                defmt::info!("Mode: {}", MODE);
            }
        }
    }
}

fn set_state(charging: bool, standby: bool) {
    let state = match (charging, standby) {
        (true, false) => ChargingState::InProgress,
        (false, true) => ChargingState::Finished,
        (false, false) => ChargingState::BatteryProblem,
        _ => ChargingState::Unknown,
    };
    unsafe { STATE = state };
}

fn set_charging(pin: &ExtiInput<'static>) {
    let low = pin.is_low();
    unsafe {
        if CHARGING != low {
            CHARGING = low;
            set_state(CHARGING, STANDBY);
            defmt::info!("State = {:?}", STATE);
        }
    }
}

#[embassy_executor::task]
async fn charge_task(mut pin: ExtiInput<'static>) {
    Timer::after_millis(200).await;
    set_charging(&pin);

    loop {
        pin.wait_for_any_edge().await;

        // Debounce
        Timer::after_millis(50).await;
        set_charging(&pin);
    }
}

fn set_standby(pin: &ExtiInput<'static>) {
    let low = pin.is_low();
    unsafe {
        if STANDBY != low {
            STANDBY = low;
            set_state(CHARGING, STANDBY);
            defmt::info!("State = {:?}", STATE);
        }
    }
}

#[embassy_executor::task]
async fn standby_task(mut pin: ExtiInput<'static>) {
    Timer::after_millis(200).await;
    set_standby(&pin);

    loop {
        pin.wait_for_any_edge().await;

        // Debounce
        Timer::after_millis(50).await;
        set_standby(&pin);
    }
}

async fn discrete_colors(led: &mut Led<'static, 1>) {
    let colors = [
        RGB8::new(MAX_BRIGHTNESS, 0, 0),
        RGB8::new(MAX_BRIGHTNESS, MAX_BRIGHTNESS, 0),
        RGB8::new(0, MAX_BRIGHTNESS, 0),
        RGB8::new(0, MAX_BRIGHTNESS, MAX_BRIGHTNESS),
        RGB8::new(0, 0, MAX_BRIGHTNESS),
        RGB8::new(MAX_BRIGHTNESS, 0, MAX_BRIGHTNESS),
        RGB8::new(0, 0, 0),
    ];
    for color in colors {
        led.write([color]);
        Timer::after_secs(1).await;
    }
}

async fn fading(led: &mut Led<'static, 1>, inc: &mut bool, value: &mut u8, color: impl Fn(u8) -> RGB8) {
    let mc = 2 * 1000_000 / (2 * MAX_BRIGHTNESS as u64);
    if *inc {
        while *value < MAX_BRIGHTNESS {
            *value += 1;
            led.write([color(*value)]);
            Timer::after_micros(mc).await;
        }
        *inc = false;
    }
    if !*inc {
        while *value > 0 {
            *value -= 1;
            led.write([color(*value)]);
            Timer::after_micros(mc).await;
        }
        *inc = true;
    }
}

#[embassy_executor::task]
async fn led_task(mut led: Led<'static, 1>) {
    led.write([RGB8::new(0, 0, 0)]);
    //discrete_colors(&mut led).await;
    Timer::after_secs(5).await;
    loop {
        unsafe {
            if STATE == ChargingState::InProgress {
                fading(&mut led, &mut true, &mut 0, |v| RGB8::new(v, 0, 0)).await;
            } else {
                led.write([RGB8::new(0, 0, 0)]);
            }
        }
    }
}

#[embassy_executor::task]
async fn stimulator_task(mut electrodes: [Output<'static>; NUM_ELECTRODES]) {
    loop {
        let program = unsafe { PROGRAMS[SELECTED_PROGRAM] };
        for stage in program.iter() {
            for (el, st) in electrodes.iter_mut().zip(stage.iter()) {
                if *st {
                    el.set_low();
                } else {
                    el.set_high();
                }
            }
            let delay = unsafe { (MODE + 1) * 5 } as u64;
            Timer::after_millis(delay).await;
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut c = Config::default();
    {
        // Import RCC configuration items to avoid long paths
        use embassy_stm32::rcc::*;

        c.rcc.hsi = true;

        // Configure the main PLL (PLL1 in some families)
        c.rcc.pll = Some(Pll {
            src: PllSource::HSI,
            prediv: PllPreDiv::DIV2,     // 8 MHz / 2 = 4 MHz
            mul: PllMul::MUL4,         // 4 MHz * 4 = 16 MHz (VCO)
        });

        // Set the System Clock source to the PLL output (PLL1_P)
        c.rcc.sys = Sysclk::PLL1_P;

        // Configure prescalers for AHB and APB buses
        c.rcc.ahb_pre = AHBPrescaler::DIV1;
        c.rcc.apb1_pre = APBPrescaler::DIV1;
        c.rcc.apb2_pre = APBPrescaler::DIV1;
    }
    let p = embassy_stm32::init(c);

    let button = ExtiInput::new(p.PA1, p.EXTI1, Pull::Up);

    let el1 = Output::new(p.PA2, Level::High, Speed::Medium);
    let el2 = Output::new(p.PA3, Level::High, Speed::Medium);
    // let el3 = Output::new(p.PB14, Level::High, Speed::Medium);
    // let el4 = Output::new(p.PA8, Level::High, Speed::Medium);

    let charge = ExtiInput::new(p.PB3, p.EXTI3, Pull::Up);
    let standby = ExtiInput::new(p.PB4, p.EXTI4, Pull::Up);

    let led = Led::new_spi(p.SPI2, p.PB15, p.DMA1_CH5);

    spawner.spawn(button_task(button)).unwrap();
    spawner.spawn(standby_task(standby)).unwrap();
    spawner.spawn(charge_task(charge)).unwrap();
    spawner.spawn(led_task(led)).unwrap();
    spawner.spawn(stimulator_task([el1, el2/*, el3, el4*/])).unwrap();

    unsafe { poll_non_sleeping(spawner) }
}

unsafe fn poll_non_sleeping(spawner: Spawner) -> ! {
    use embassy_executor::raw::Executor;

    let executor = unsafe {
        &mut *(spawner.executor_id() as *const Executor as *mut Executor)
    };

    loop {
        unsafe {
            executor.poll();
        }
        cortex_m::asm::nop();
    }
}