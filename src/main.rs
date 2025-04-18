#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, i2c, peripherals};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
use panic_probe as _;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    bind_interrupts!(struct Irqs {
        I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
        I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
    });

    let i2c = embassy_stm32::i2c::I2c::new(
        p.I2C1,
        p.PB6,
        p.PB7,
        Irqs,
        p.DMA1_CH6,
        p.DMA1_CH7,
        Hertz::khz(400),
        Default::default(),
    );

    let interface = I2CDisplayInterface::new(i2c);
    let mut display =
        Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0).into_terminal_mode();
    display.init().unwrap();
    let _ = display.clear();
    let _ = display.print_char('H');
    let _ = display.print_char('e');
    let _ = display.print_char('l');
    let _ = display.print_char('l');
    let _ = display.print_char('o');

    let mut led = Output::new(p.PC13, Level::High, Speed::Low);
    
    // let _ = display.write_fmt(format_args!("{}", 10));
    loop {
        led.set_high();
        Timer::after_millis(30).await;

        led.set_low();
        Timer::after_millis(30).await;
    }
}