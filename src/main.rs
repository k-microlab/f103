#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Input, Level, Output, OutputType, Pull, Speed};
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::Channel;
use embassy_stm32::timer::low_level::CountingMode;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_time::Timer;
use embedded_hal::Pwm;
use panic_probe as _;
use ssd1306::prelude::*;

use manchester_code::{ActivityLevel, SyncOnTurningEdge, BitOrder, Decoder, Encoder, InfraredEmitter, Datagram, DatagramBigEndianIterator};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    // let mut led = Output::new(p.PC13, Level::High, Speed::Low);
    let pwm_pin = PwmPin::new_ch1(p.PA8, OutputType::PushPull);
    let mut pwm = SimplePwm::new(p.TIM1, Some(pwm_pin), None, None, None, Hertz::khz(36), CountingMode::default());
    pwm.set_duty(Channel::Ch1, pwm.get_max_duty() / 4);
    const PAUSE_US: u64 = 889;

    Timer::after_micros(PAUSE_US).await;

    const PAUSE_HALF_BITS_BETWEEN_DATAGRAMS: u8 = 3;

    let mut infrared_emitter = InfraredEmitter::<_, _, DatagramBigEndianIterator>::new(PAUSE_HALF_BITS_BETWEEN_DATAGRAMS, pwm, Channel::Ch1);

    defmt::println!("Init done");

    let datagram = Datagram::new("0101_0011_0111_0001");

    loop {
        defmt::println!("Send new datagram {}", datagram);
        infrared_emitter.send_if_possible(datagram, 25);

        for _ in 0..32 {
            infrared_emitter.send_half_bit();
            Timer::after_micros(PAUSE_US).await;
        }
    }
}