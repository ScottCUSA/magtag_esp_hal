#![no_std]
#![no_main]

use blocking_network_stack::Stack;
use core::net::Ipv4Addr;
use embedded_graphics::{
    pixelcolor::Gray2,
    prelude::*,
    primitives::{Primitive, PrimitiveStyle, Rectangle},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_io::{Read as _, Write as _};
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig},
    main, ram,
    rng::Rng,
    spi::{self, master::Spi},
    time::{self, Duration, Rate},
    timer::timg::TimerGroup,
};
use esp_println::logger::init_logger;
use esp_radio::wifi::{ClientConfig, ModeConfig, ScanConfig};
use log::info;
use smoltcp::{
    iface::{SocketSet, SocketStorage},
    wire::{DhcpOption, IpAddress},
};
use ssd1680::displays::adafruit_thinkink_2in9::{Display2in9Gray2, ThinkInk2in9Gray2};
use ssd1680::prelude::*;

esp_bootloader_esp_idf::esp_app_desc!();

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

#[main]
fn main() -> ! {
    // Initialize logger for esp-println
    init_logger(log::LevelFilter::Info);

    info!("Initialize peripherals");
    // Setup CPU clock and watchdog, returns the peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    let esp_radio_ctrl = esp_radio::init().unwrap();

    let (mut controller, interfaces) =
        esp_radio::wifi::new(&esp_radio_ctrl, peripherals.WIFI, Default::default()).unwrap();

    let mut device = interfaces.sta;
    let iface = create_interface(&mut device);

    let mut socket_set_entries: [SocketStorage; 3] = Default::default();
    let mut socket_set = SocketSet::new(&mut socket_set_entries[..]);
    let mut dhcp_socket = smoltcp::socket::dhcpv4::Socket::new();
    // we can set a hostname here (or add other DHCP options)
    dhcp_socket.set_outgoing_options(&[DhcpOption {
        kind: 12,
        data: b"esp-radio",
    }]);
    socket_set.add(dhcp_socket);

    let rng = Rng::new();
    let now = || time::Instant::now().duration_since_epoch().as_millis();
    let stack = Stack::new(iface, device, socket_set, now, rng.random());

    controller
        .set_power_saving(esp_radio::wifi::PowerSaveMode::None)
        .unwrap();

    let client_config = ModeConfig::Client(
        ClientConfig::default()
            .with_ssid(SSID.into())
            .with_password(PASSWORD.into()),
    );
    let res = controller.set_config(&client_config);
    info!("wifi_set_configuration returned {:?}", res);

    controller.start().unwrap();
    info!("is wifi started: {:?}", controller.is_started());

    info!("Start Wifi Scan");
    let scan_config = ScanConfig::default().with_max(10);
    let res = controller.scan_with_config(scan_config).unwrap();
    for ap in res {
        info!("{:?}", ap);
    }

    info!("{:?}", controller.capabilities());
    info!("wifi_connect {:?}", controller.connect());

    // wait to get connected
    info!("Wait to get connected");
    loop {
        match controller.is_connected() {
            Ok(true) => break,
            Ok(false) => {}
            Err(err) => {
                info!("{:?}", err);
                loop {}
            }
        }
    }
    info!("{:?}", controller.is_connected());

    // wait for getting an ip address
    info!("Wait to get an ip address");
    loop {
        stack.work();

        if stack.is_iface_up() {
            info!("got ip {:?}", stack.get_ip_info());
            break;
        }
    }

    info!("Start busy loop on main");

    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    let mut socket = stack.get_socket(&mut rx_buffer, &mut tx_buffer);

    info!("Making HTTP request");
    socket.work();

    socket
        .open(IpAddress::Ipv4(Ipv4Addr::new(142, 250, 185, 115)), 80)
        .unwrap();

    socket
        .write(b"GET / HTTP/1.0\r\nHost: www.mobile-j.de\r\n\r\n")
        .unwrap();
    socket.flush().unwrap();

    let deadline = time::Instant::now() + Duration::from_secs(20);
    let mut buffer = [0u8; 512];
    while let Ok(len) = socket.read(&mut buffer) {
        let to_print = unsafe { core::str::from_utf8_unchecked(&buffer[..len]) };
        info!("{}", to_print);

        if time::Instant::now() > deadline {
            info!("Timeout");
            break;
        }
    }

    socket.disconnect();

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

    // Create display with SPI interface
    let mut epd = ThinkInk2in9Gray2::new(spi_device, busy, dc, rst).unwrap();
    let mut display_gray = Display2in9Gray2::new();

    // Initialize the display
    epd.begin(&mut Delay::new()).unwrap();

    info!("Draw some black text");
    let character_style = embedded_graphics::mono_font::MonoTextStyle::new(
        &embedded_graphics::mono_font::ascii::FONT_7X14_BOLD,
        Gray2::BLACK,
    );
    embedded_graphics::text::Text::new(
        "Hello from Gray2 Rust!",
        Point::new(10, 15),
        character_style,
    )
    .draw(&mut display_gray)
    .unwrap();

    info!("Draw a light gray cube");
    Rectangle::new(Point::new(50, 50), Size::new(25, 25))
        .into_styled(PrimitiveStyle::with_fill(Gray2::new(0x01)))
        .draw(&mut display_gray)
        .unwrap();

    info!("Draw dark gray bitmap");
    // Create an ImageRaw from raw bytes (1bpp) and draw it; adjust the width to match the bitmap width
    let raw = embedded_graphics::image::ImageRaw::<embedded_graphics::pixelcolor::BinaryColor>::new(
        &include_bytes!("../../assets/ferris.bin")[..],
        100,
    );
    embedded_graphics::image::Image::new(&raw, Point::new(100, 20))
        .draw(&mut display_gray.as_binary_draw_target())
        .unwrap();

    info!("Draw a black line");
    let line = embedded_graphics::primitives::Line::new(Point::new(200, 20), Point::new(240, 107));
    line.into_styled(PrimitiveStyle::with_stroke(Gray2::BLACK, 2))
        .draw(&mut display_gray)
        .unwrap();

    info!("Display frame");
    // Transfer and display the buffer on the display
    epd.update_gray2_and_display(
        display_gray.high_buffer(),
        display_gray.low_buffer(),
        &mut Delay::new(),
    )
    .unwrap();

    // Done
    info!("Done");
    loop {
        let deadline = time::Instant::now() + Duration::from_secs(5);
        while time::Instant::now() < deadline {
            socket.work();
        }
    }
}

// some smoltcp boilerplate
fn timestamp() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(
        esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_micros() as i64,
    )
}

pub fn create_interface(device: &mut esp_radio::wifi::WifiDevice) -> smoltcp::iface::Interface {
    // users could create multiple instances but since they only have one WifiDevice
    // they probably can't do anything bad with that
    smoltcp::iface::Interface::new(
        smoltcp::iface::Config::new(smoltcp::wire::HardwareAddress::Ethernet(
            smoltcp::wire::EthernetAddress::from_bytes(&device.mac_address()),
        )),
        device,
        timestamp(),
    )
}
