#![no_std]
#![no_main]

use panic_probe as _;
use defmt_rtt as _;

use cortex_m_rt::{entry, exception, ExceptionFrame};
use embedded_graphics::{geometry::Point, image::Image, pixelcolor::Rgb565, prelude::*};
use embedded_graphics::primitives::Rectangle;
use ssd1331::{DisplayRotation, Ssd1331};
use stm32f1xx_hal::{
    prelude::*,
    spi::{Mode, Phase, Polarity, Spi},
    stm32,
};
use tinybmp::Bmp;

#[entry]
fn main() -> ! {
    let cp = cortex_m::Peripherals::take().unwrap();
    let dp = stm32::Peripherals::take().unwrap();
    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();
    let clocks = rcc.cfgr.freeze(&mut flash.acr);
    let mut afio = dp.AFIO.constrain();
    let mut gpioa = dp.GPIOA.split();
    let mut gpiob = dp.GPIOB.split();

    // SPI1
    let sck = gpioa.pa5.into_alternate_push_pull(&mut gpioa.crl);
    let miso = gpioa.pa6;
    let mosi = gpioa.pa7.into_alternate_push_pull(&mut gpioa.crl);
    let mut delay = cp.SYST.delay(&clocks);
    let mut rst = gpiob.pb0.into_push_pull_output(&mut gpiob.crl);
    let dc = gpiob.pb1.into_push_pull_output(&mut gpiob.crl);

    let spi = Spi::spi1(
        dp.SPI1,
        (sck, miso, mosi),
        &mut afio.mapr,
        Mode {
            polarity: Polarity::IdleLow,
            phase: Phase::CaptureOnFirstTransition,
        },
        256.kHz(),
        clocks
    );

    let mut disp = Ssd1331::new(spi, dc, DisplayRotation::Rotate0);

    disp.reset(&mut rst, &mut delay).unwrap();
    disp.init().unwrap();
    disp.flush().unwrap();

    let (w, h) = disp.dimensions();

    defmt::info!("Display size: {:?}x{:?}", w, h);

    disp.set_draw_area((0, 0), (w, h)).expect("Failed to set draw area");
    // disp.fill_solid(&Rectangle::new(Point::new(0, 0), Size::new(w as _, h as _)), Rgb565::GREEN).expect("Failed to fill solid area");
    <_ as DrawTarget>::clear(&mut disp, Rgb565::GREEN).expect("Failed to clear display");

    /*let bmp =
        Bmp::from_slice(include_bytes!("./image.bmp")).expect("Failed to load BMP image");

    let im: Image<Bmp<Rgb565>> = Image::new(&bmp, Point::zero());

    // Position image in the center of the display
    let moved = im.translate(Point::new(
        (w as u32 - bmp.size().width) as i32 / 2,
        (h as u32 - bmp.size().height) as i32 / 2,
    ));

    moved.draw(&mut disp).unwrap();*/

    disp.flush().unwrap();

    loop {}
}

#[exception]
unsafe fn HardFault(ef: &ExceptionFrame) -> ! {
    panic!("{:#?}", ef);
}