//! Demonstrates how to use SSD1331 display on STM32F103 (Blue Pill, Maple,
//! etc).
//!
//! Connections:
//!
//! | Display | MCU   |
//! |---------|-------|
//! | GND     | GND   |
//! | VCC     | 3.3V  |
//! | SCL     | PA5   |
//! | SDA     | PA7   |
//! | RES     | PA0   |
//! | DC      | PC15  |
//! | CS      | PC14  |
//!
//! Assuming you have a debug probe connected to your board and probe-rs tools
//! installed, running the example with cargo should program the board. Note
//! that dev build may not fit into the flash memory of STM32F103C8.
//!
//! ```sh
//! $ cargo run --release --example main
//! ...
//! 0.005401 INFO  image copy: 2349 us
//! 0.007385 INFO  font render: 1953 us
//! 0.010375 INFO  graphics render: 3295 us
//! ...
//! ```
//!
//! The font file used in this example (font_6x12.bin) is FONT_6X12 from
//! embedded-graphics, reformatted to character-major order.

#![no_std]
#![no_main]

use cortex_m_rt::exception;
use defmt::{error, info};
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::{gpio, spi};
use embassy_sync::blocking_mutex::NoopMutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X12, MonoTextStyle},
    pixelcolor::{raw::ToBytes, Rgb565},
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle, Triangle},
    text::Text,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use ssd1331_async::{BitDepth, Config, Framebuffer, Ssd1331, WritePixels};
use static_cell::ConstStaticCell;

use {defmt_rtt as _, panic_probe as _};

const FRAME_BUFFER_SIZE: usize = 32 * 40 * 2;
static PIXEL_DATA: ConstStaticCell<[u8; FRAME_BUFFER_SIZE]> =
    ConstStaticCell::new([0; FRAME_BUFFER_SIZE]);

fn fast_config() -> embassy_stm32::Config {
    let mut cfg = embassy_stm32::Config::default();
    cfg.rcc.hse = Some(embassy_stm32::rcc::Hse {
        freq: embassy_stm32::time::Hertz(8_000_000),
        mode: embassy_stm32::rcc::HseMode::Oscillator,
    });
    cfg.rcc.apb1_pre = embassy_stm32::rcc::APBPrescaler::DIV2;
    cfg.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_P;
    cfg.rcc.pll = Some(embassy_stm32::rcc::Pll {
        src: embassy_stm32::rcc::PllSource::HSE,
        prediv: embassy_stm32::rcc::PllPreDiv::DIV1,
        mul: embassy_stm32::rcc::PllMul::MUL9,
    });
    cfg
}

struct TextRenderer {
    data: &'static [u8],
    char_size: Size,
    char_byte_count: usize,
}

impl TextRenderer {
    pub fn new(data: &'static [u8], char_size: Size) -> Self {
        let char_bit_count = char_size.width as usize * char_size.height as usize;
        assert!(char_bit_count % 8 == 0);
        Self {
            data,
            char_size,
            char_byte_count: char_bit_count / 8,
        }
    }

    fn unpack(&self, c: char, buf: &mut [u8], fc: &[u8], bc: &[u8]) {
        assert!(fc.len() == bc.len());
        let color_len = fc.len();
        let idx = c as usize - ' ' as usize;
        let start = idx * self.char_byte_count;
        let mut i = 0;
        for b in &self.data[start..start + self.char_byte_count] {
            let mut code = *b;
            for _ in 0..8 {
                buf[i..i + color_len].copy_from_slice(if code & 1 == 1 { fc } else { bc });
                code >>= 1;
                i += color_len;
            }
        }
    }

    pub async fn render_text(
        &self,
        text: &str,
        top_left: Point,
        fc: Rgb565,
        bc: Rgb565,
        buf: &mut [u8],
        display: &mut impl WritePixels,
    ) {
        let buf_size = self.char_size.width as usize * self.char_size.height as usize * 2;
        let buf = &mut buf[..buf_size];
        for (i, c) in text.chars().enumerate() {
            self.unpack(c, buf, fc.to_be_bytes().as_ref(), bc.to_be_bytes().as_ref());
            display
                .write_pixels(
                    buf,
                    BitDepth::Sixteen,
                    Rectangle::new(
                        top_left + Point::new(i as i32 * self.char_size.width as i32, 0),
                        self.char_size,
                    ),
                )
                .await;
        }
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut p = embassy_stm32::init(fast_config());

    let mut spi_config = spi::Config::default();
    spi_config.frequency = embassy_stm32::time::Hertz(50_000_000);
    let spi_bus = Mutex::<NoopRawMutex, _>::new(spi::Spi::new_txonly(
        &mut p.SPI1,
        &mut p.PA5,
        &mut p.PA7,
        &mut p.DMA1_CH3,
        spi_config,
    ));
    let mut display = {
        let cs = gpio::Output::new(&mut p.PC14, gpio::Level::Low, gpio::Speed::VeryHigh);
        let spi_dev = SpiDevice::new(&spi_bus, cs);

        let rst = gpio::Output::new(&mut p.PA0, gpio::Level::Low, gpio::Speed::VeryHigh);
        let dc = gpio::Output::new(&mut p.PC15, gpio::Level::Low, gpio::Speed::VeryHigh);

        Ssd1331::new(Config::default(), rst, dc, spi_dev, &mut Delay {})
            .await
            .unwrap()
    };

    // Copy an image from flash to the display. You can convert images to
    // the appropriate format using something like:
    // ```sh
    // ffmpeg -i in.gif -vcodec rawvideo -f rawvideo -pix_fmt rgb565be out.raw
    // ```
    let img = include_bytes!("./img.raw");
    let start = Instant::now();
    display
        .write_pixels(
            img,
            BitDepth::Sixteen,
            Rectangle::new(Point::new(32, 0), Size::new(64, 64)),
        )
        .await
        .unwrap();
    info!(
        "image copy: {} us",
        Instant::now().duration_since(start).as_micros()
    );

    // Use the first 12x6x2 bytes of the static buffer to render text
    // character by character and transfer it to the screen. If we couldn't
    // spare 144 bytes, we could do this in even smaller chunks.
    let pixel_data = PIXEL_DATA.take();
    let font = TextRenderer::new(include_bytes!("./font_6x12.bin"), Size::new(6, 12));
    let start = Instant::now();
    font.render_text(
        "Hello",
        Point::zero(),
        Rgb565::CSS_FLORAL_WHITE,
        Rgb565::CSS_INDIGO,
        pixel_data,
        &mut display,
    )
        .await;
    font.render_text(
        "Rust!",
        Point::new(0, 12),
        Rgb565::CSS_FLORAL_WHITE,
        Rgb565::CSS_INDIGO,
        pixel_data,
        &mut display,
    )
        .await;
    info!(
        "font render: {} us",
        Instant::now().duration_since(start).as_micros()
    );

    // Create an Rgb565 32x40 framebuffer and use embedded-graphics to draw
    // some shapes and text with transparent background. Then transfer the
    // framebuffer to the screen.
    let start = Instant::now();
    let mut fb = Framebuffer::<Rgb565>::new(pixel_data, Size::new(32, 40));
    fb.clear(Rgb565::BLACK).unwrap();
    Circle::new(Point::new(2, 6), 28)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_DARK_ORANGE))
        .draw(&mut fb)
        .unwrap();
    Triangle::new(Point::new(5, 13), Point::new(26, 13), Point::new(16, 31))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CSS_BLUE))
        .draw(&mut fb)
        .unwrap();
    Text::new(
        "eg",
        Point::new(10, 20),
        MonoTextStyle::new(&FONT_6X12, Rgb565::CSS_WHITE),
    )
        .draw(&mut fb)
        .unwrap();
    display.flush(&fb, Point::new(0, 24)).await;
    info!(
        "graphics render: {} us",
        Instant::now().duration_since(start).as_micros()
    );

    loop {
        Timer::after(Duration::from_millis(1000)).await;
        info!("ping");
    }
}

#[allow(non_snake_case)]
#[exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    error!("HardFault at {:#010x}", ef.pc());
    loop {
        cortex_m::asm::nop();
    }
}