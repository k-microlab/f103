#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::Config;
use embassy_stm32::spi::{Config as SpiConfig, MODE_0, MODE_1};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::spi::Spi;
use embassy_stm32::time::Hertz;
use embassy_time::Timer;
use panic_probe as _;
use smart_leds::{gamma, SmartLedsWrite, RGB8};
use ws2812_spi::Ws2812;

const NUM_ELECTRODES: usize = 2;
const NUM_PROGRAMS: usize = 1;

const H: bool = true;
const L: bool = false;


const MODES: usize = 5;
static mut MODE: usize = 0;

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

        if button.is_low() {
            unsafe {
                MODE = (MODE + 1) % MODES;
                defmt::info!("Mode: {}", MODE);
            }
        }
    }
}

async fn discrete_colors(led: &mut Ws2812<Spi<'static, Async>>) {
    let colors = [
        RGB8::new(255, 0, 0),
        RGB8::new(255, 255, 0),
        RGB8::new(0, 255, 0),
        RGB8::new(0, 255, 255),
        RGB8::new(0, 0, 255),
        RGB8::new(255, 0, 255),
        RGB8::new(0, 0, 0),
    ];
    for color in colors {
        led.write(gamma([color].into_iter())).unwrap();
        Timer::after_secs(1).await;
    }
}

async fn fading(led: &mut Ws2812<Spi<'static, Async>>) {
    let mut inc = true;
    let mut v = 0;
    loop {
        if inc {
            while v < 255 {
                v += 1;
                let color = RGB8::new(v, 0, 0);
                led.write(gamma([color].into_iter())).unwrap();
                Timer::after_millis(5).await;
            }
        } else {
            while v > 0 {
                v -= 1;
                let color = RGB8::new(v, 0, 0);
                led.write(gamma([color].into_iter())).unwrap();
                Timer::after_millis(5).await;
            }
        }
        inc = !inc;
    }
}

#[embassy_executor::task]
async fn led_task(mut led: Ws2812<Spi<'static, Async>>) {
    discrete_colors(&mut led).await;
    Timer::after_secs(5).await;
    fading(&mut led).await;
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

        // Enable HSE and set its frequency and mode
        c.rcc.hse = Some(Hse {
            freq: Hertz(8_000_000), // Specify your crystal frequency
            mode: HseMode::Oscillator, // Use HseMode::Bypass if using an external clock source
        });

        // Configure the main PLL (PLL1 in some families)
        c.rcc.pll = Some(Pll {
            src: PllSource::HSE,
            prediv: PllPreDiv::DIV1,     // 8 MHz / 1 = 8 MHz
            mul: PllMul::MUL9,         // 8 MHz * 9 = 72 MHz (VCO)
        });

        // Set the System Clock source to the PLL output (PLL1_P)
        c.rcc.sys = Sysclk::PLL1_P;

        // Configure prescalers for AHB and APB buses
        c.rcc.ahb_pre = AHBPrescaler::DIV1;
        c.rcc.apb1_pre = APBPrescaler::DIV2;
        c.rcc.apb2_pre = APBPrescaler::DIV1;
    }
    let p = embassy_stm32::init(c);

    let button = ExtiInput::new(p.PA1, p.EXTI1, Pull::Up);

    let el1 = Output::new(p.PA2, Level::High, Speed::Medium);
    let el2 = Output::new(p.PA3, Level::High, Speed::Medium);
    // let el3 = Output::new(p.PB14, Level::High, Speed::Medium);
    // let el4 = Output::new(p.PA8, Level::High, Speed::Medium);

    let mut config = SpiConfig::default();
    config.frequency = Hertz(2_000_000);
    config.mode = MODE_1;
    let spi = Spi::new_txonly_nosck(p.SPI2, p.PB15, p.DMA1_CH5, config);

    let led = Ws2812::new(spi);

    spawner.spawn(button_task(button)).unwrap();
    spawner.spawn(led_task(led)).unwrap();
    // spawner.spawn(stimulator_task([el1, el2/*, el3, el4*/])).unwrap();

    let executor = unsafe {
        &mut *(spawner.executor_id() as *const embassy_executor::raw::Executor as *mut embassy_executor::raw::Executor)
    };

    loop {
        unsafe {
            executor.poll();
        }
        cortex_m::asm::nop();
    }
}