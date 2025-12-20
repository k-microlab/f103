use embassy_stm32::mode::Async;
use embassy_stm32::Peri;
use embassy_stm32::spi::{Config as SpiConfig, Instance as SpiInstance, MosiPin, Spi, TxDma, MODE_1};
use embassy_stm32::time::Hertz;
use smart_leds::{gamma, SmartLedsWrite, RGB8};
use ws2812_spi::Ws2812;

pub struct Led<'d, const N: usize> {
    inner: Ws2812<Spi<'d, Async>>
}

impl<'d, const N: usize> Led<'d, N> {
    pub fn new_spi<T: SpiInstance>(
        peri: Peri<'d, T>,
        mosi: Peri<'d, impl MosiPin<T>>,
        tx_dma: Peri<'d, impl TxDma<T>>,
    ) -> Self {
        let mut config = SpiConfig::default();
        config.frequency = Hertz(2_000_000);
        config.mode = MODE_1;
        let spi = Spi::new_txonly_nosck(peri, mosi, tx_dma, config);
        let inner = Ws2812::new(spi);

        Self {
            inner
        }
    }

    pub fn write(&mut self, colors: [RGB8; N]) {
        self.inner.write(gamma(colors.into_iter())).unwrap();
    }
}