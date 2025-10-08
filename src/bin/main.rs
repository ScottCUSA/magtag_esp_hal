#![no_std]
#![no_main]

use embedded_graphics::{
    prelude::*,
    primitives::{Primitive, PrimitiveStyle, Rectangle},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    main,
    spi::{self, master::Spi},
    time::Rate,
};
use esp_println::logger::init_logger;
use log::info;
use ssd1680::{color::Black, displays::adafruit_thinkink_2in9::ThinkInk2in9Mono};
use ssd1680::{displays::adafruit_thinkink_2in9::Display2in9, prelude::*};
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // Initialize logger for esp-println
    init_logger(log::LevelFilter::Info);

    info!("Initialize peripherals");
    // Setup CPU clock and watchdog, returns the peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // SPI display driver setup
    let sclk = peripherals.GPIO36;
    let mosi = peripherals.GPIO35;
    let miso = peripherals.GPIO37;
    let spi = Spi::new(
        peripherals.SPI2,
        spi::master::Config::default().with_frequency(Rate::from_mhz(4)),
    )
    .unwrap()
    .with_sck(sclk)
    .with_miso(miso)
    .with_mosi(mosi);
    let busy = Input::new(peripherals.GPIO5, InputConfig::default());
    let rst = Output::new(peripherals.GPIO6, Level::Low, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO7, Level::High, OutputConfig::default());
    let cs = Output::new(peripherals.GPIO8, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();

    // Create epaper display with SPI interface
    let mut epd = ThinkInk2in9Mono::new(spi_device, busy, dc, rst).unwrap();
    // Initialize the display
    epd.begin(&mut Delay::new()).unwrap();

    // setup graphics buffer
    let mut display_buffer = Display2in9::new();
    info!("Draw a rectangle");
    Rectangle::new(Point::new(50, 50), Size::new(25, 25))
        .into_styled(PrimitiveStyle::with_fill(Black))
        .draw(&mut display_buffer)
        .unwrap();
    info!("Draw some text");
    let text_style = embedded_graphics::mono_font::MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_7X14,
        Black,
    );
    embedded_graphics::text::Text::new("Hello from Rust!", Point::new(10, 15), text_style)
        .draw(&mut display_buffer)
        .unwrap();
    info!("Draw a bitmap");
    // Create an ImageRaw from raw bytes (1bpp) and draw it; adjust the width to match the bitmap width
    let raw = embedded_graphics::image::ImageRaw::<embedded_graphics::pixelcolor::BinaryColor>::new(
        &include_bytes!("../../assets/ferris.bin")[..],
        100,
    );
    embedded_graphics::image::Image::new(&raw, Point::new(100, 20))
        .draw(&mut display_buffer)
        .unwrap();
    info!("Draw a line");
    let line = embedded_graphics::primitives::Line::new(Point::new(200, 20), Point::new(240, 107));
    line.into_styled(PrimitiveStyle::with_stroke(Black, 1))
        .draw(&mut display_buffer)
        .unwrap();

    info!("Display frame");
    // Transfer and display the buffer on the display
    epd.update_and_display(display_buffer.buffer(), &mut Delay::new())
        .unwrap();

    // Done
    info!("Done");
    let delay = Delay::new();
    loop {
        delay.delay_millis(1);
    }
}
