#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::{bind_interrupts, Config};
use embassy_stm32::crc::Crc;
use embassy_stm32::dma::NoDma;
use embassy_stm32::gpio::{Input, Level, Output, OutputType, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::Channel;
use embassy_stm32::timer::low_level::CountingMode;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_stm32::peripherals;
use embassy_stm32::usart;
use embassy_stm32::usart::{Config as UartConfig, UartRx, UartTx};
use embassy_time::Timer;
use embedded_hal::prelude::_embedded_hal_blocking_spi_Write;
use panic_probe as _;

bind_interrupts!(struct Irqs {
    USART1 => usart::InterruptHandler<peripherals::USART1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: Hertz(8_000_000),
            // Oscillator for bluepill, Bypass for nucleos.
            mode: HseMode::Oscillator,
        });
        config.rcc.pll = Some(Pll {
            src: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL9,
        });
        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV1;
    }
    let p = embassy_stm32::init(config);
    let mut crc = Crc::new(p.CRC);

    let mut config = UartConfig::default();
    config.baudrate = 1200;
    let mut rx = UartRx::new(p.USART1, Irqs, p.PA10, p.DMA1_CH5, config).unwrap();

    info!("uart init");

    let mut buffer = Buffer::<255>::new();

    loop {
        if let Ok(Some(packet)) = buffer.read_packet(&mut rx, &mut crc).await {
            match core::str::from_utf8(packet.data) {
                Ok(s) => {
                    info!("Packet: \"{}\"", s);
                }
                Err(e) => {
                    info!("Packet: <Invalid UTF-8 string>");
                }
            }
        }
    }
}

struct Packet<'a> {
    data: &'a [u8],
    crc: u32,
}

struct Buffer<const N: usize> {
    buffer: [u8; N],
    position: u8
}

impl<const N: usize> Buffer<N> {
    fn new() -> Self {
        Self {
            buffer: [0; N],
            position: 0,
        }
    }

    fn last(&self) -> u8 {
        self.buffer[self.position as usize]
    }

    fn next(&mut self) -> &mut u8 {
        self.position += 1;
        if self.position as usize >= self.buffer.len() {
            self.reset();
        }
        let slot = &mut self.buffer[self.position as usize];
        slot
    }

    fn reset(&mut self) {
        self.position = 0;
    }

    async fn read_packet<'a>(&'a mut self, rx: &mut UartRx<'_, Async>, crc: &mut Crc<'_>) -> Result<Option<Packet<'a>>, usart::Error> {
        let last = self.last();
        let slot = self.next();
        rx.read(core::slice::from_mut(slot)).await?;
        if *slot == last && last == 0b01010101 {
            self.reset();
            let mut len = 0;
            rx.read(core::slice::from_mut(&mut len)).await?;
            info!("Incoming packet: {} bytes", len);
            let data = &mut self.buffer[0..len as usize];
            rx.read(data).await?;
            let mut crc = [0u8; 4];
            rx.read(&mut crc).await?;
            Ok(Some(Packet {
                data,
                crc: u32::from_be_bytes(crc),
            }))
        } else {
            Ok(None)
        }
    }
}

async fn write_packet(tx: &mut UartTx<'_, Async>, packet: &[u8], crc: &mut Crc<'_>) -> Result<(), usart::Error> {
    let crc = calc_crc(packet, crc);
    tx.write(&[0b01010101, 0b01010101]).await?;
    tx.write(&[packet.len() as u8]).await?;
    tx.write(packet).await?;
    tx.write(&u32::to_be_bytes(crc)).await?;
    Ok(())
}

fn calc_crc(data: &[u8], crc: &mut Crc) -> u32 {
    crc.reset();
    let (words, tail) = data.as_chunks::<4>();
    for word in words {
        let word = u32::from_be_bytes(*word);
        crc.feed_word(word);
    }
    match tail.len() {
        3 => {
            let word = u32::from_be_bytes([tail[0], tail[1], tail[2], 0]);
            crc.feed_word(word);
        }
        2 => {
            let word = u32::from_be_bytes([tail[0], tail[1], 0, 0]);
            crc.feed_word(word);
        }
        1 => {
            let word = u32::from_be_bytes([tail[0], 0, 0, 0]);
            crc.feed_word(word);
        }
        _ => {}
    }
    crc.read()
}