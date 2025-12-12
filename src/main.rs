#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::dma::word::U5;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_time::{Duration, Timer};
use embedded_hal::digital::{OutputPin};
use panic_probe as _;

const NUM_ELECTRODES: usize = 4;
const NUM_PROGRAMS: usize = 1;

const NR_PIXELS: usize = 1;
const BITS_PER_PIXEL: usize = 24; // 24 for rgb, 32 for rgbw
const TOTAL_BITS: usize = NR_PIXELS * BITS_PER_PIXEL;

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

struct RGB {
    r: u8,
    g: u8,
    b: u8,
}
impl Default for RGB {
    fn default() -> RGB {
        RGB { r: 0, g: 0, b: 0 }
    }
}
pub struct Ws2812 {
    // Note that the U5 type controls the selection of 5 bits to output
    bitbuffer: [U5; TOTAL_BITS],
}

impl Ws2812 {
    pub fn new() -> Ws2812 {
        Ws2812 {
            bitbuffer: [U5(0); TOTAL_BITS],
        }
    }
    fn len(&self) -> usize {
        return NR_PIXELS;
    }
    fn set(&mut self, idx: usize, rgb: RGB) {
        self.render_color(idx, 0, rgb.g);
        self.render_color(idx, 8, rgb.r);
        self.render_color(idx, 16, rgb.b);
    }
    // transform one color byte into an array of 8 byte. Each byte in the array does represent 1 neopixel bit pattern
    fn render_color(&mut self, pixel_idx: usize, offset: usize, color: u8) {
        let mut bits = color as usize;
        let mut idx = pixel_idx * BITS_PER_PIXEL + offset;

        // render one bit in one spi byte. High time first, then the low time
        // clock should be 4 Mhz, 5 bits, each bit is 0.25 us.
        // a one bit is send as a pulse of 0.75 high -- 0.50 low
        // a zero bit is send as a pulse of 0.50 high -- 0.75 low
        // clock frequency for the neopixel is exact 800 khz
        // note that the mosi output should have a resistor to ground of 10k,
        // to assure that between the bursts the line is low
        for _i in 0..8 {
            if idx >= TOTAL_BITS {
                return;
            }
            let pattern = match bits & 0x80 {
                0x80 => 0b0000_1110,
                _ => 0b000_1100,
            };
            bits = bits << 1;
            self.bitbuffer[idx] = U5(pattern);
            idx += 1;
        }
    }
}

#[embassy_executor::task]
async fn button_task(mut button: ExtiInput<'static>) {
    loop {
        button.wait_for_falling_edge().await;

        // Debounce
        Timer::after(Duration::from_millis(50)).await;

        if button.is_low() {
            unsafe {
                SELECTED_PROGRAM = (SELECTED_PROGRAM + 1) % NUM_PROGRAMS;
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
    let el3 = Output::new(p.PB14, Level::High, Speed::Medium);
    let el4 = Output::new(p.PA8, Level::High, Speed::Medium);

    let mut config = Config::default();
    config.frequency = Hertz(4_000_000);
    let mut spi = Spi::new_txonly_nosck(p.SPI2, p.PB15, p.DMA1_CH5, config);

    let mut neopixels = Ws2812::new();

    spawner.spawn(button_task(button)).unwrap();
    spawner.spawn(stimulator_task([el1, el2, el3, el4])).unwrap();

    loop {
        let mut cnt: usize = 0;
        for _i in 0..10 {
            let color = match cnt % 3 {
                0 => RGB { r: 0x21, g: 0, b: 0 },
                1 => RGB { r: 0, g: 0x31, b: 0 },
                _ => RGB { r: 0, g: 0, b: 0x41 },
            };
            neopixels.set(0, color);
            cnt += 1;
            // start sending the neopixel bit patters over spi to the neopixel string
            spi.write::<u8>(unsafe { core::mem::transmute::<_, &[u8; BITS_PER_PIXEL]>(&neopixels.bitbuffer) }).await.ok();
            Timer::after_millis(500).await;
        }
        Timer::after_millis(1000).await;
    }
}