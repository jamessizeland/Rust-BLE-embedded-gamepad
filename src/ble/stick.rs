use defmt::info;
use embassy_nrf::{
    interrupt::{self, InterruptExt as _},
    peripherals::{P0_03, P0_04, SAADC},
    saadc::{self, Input as _, Saadc},
};
use embassy_time::{Duration, Timer};
use nrf_softdevice::ble::Connection;

use crate::io::Irqs;

use super::gatt::GamepadServer;

#[nrf_softdevice::gatt_service(uuid = "1812")]
pub struct StickService {
    #[characteristic(uuid = "2ae2", read, notify)]
    x: i8,
    #[characteristic(uuid = "2ae2", read, notify)]
    y: i8,
}

pub fn init_analog_adc(x_pin: P0_03, y_pin: P0_04, adc: SAADC) -> Saadc<'static, 2> {
    let config = embassy_nrf::saadc::Config::default();
    interrupt::SAADC.set_priority(interrupt::Priority::P3);
    let channel_cfg = saadc::ChannelConfig::single_ended(x_pin.degrade_saadc());
    let channel_cfg2 = saadc::ChannelConfig::single_ended(y_pin.degrade_saadc());
    saadc::Saadc::new(adc, Irqs, config, [channel_cfg, channel_cfg2])
}

struct Axis {
    offset: i16,
    divider: i16,
    old: i8,
}

impl Axis {
    fn new(offset: i16, divider: i16) -> Self {
        Self {
            offset,
            divider,
            old: 0,
        }
    }
    fn changed(&mut self, new_raw: i16) -> Option<i8> {
        let new = ((new_raw - self.offset) / 370) as i8;
        if new != self.old {
            self.old = new;
            Some(new as i8)
        } else {
            None
        }
    }
}

pub async fn analog_stick_task(
    server: &GamepadServer,
    connection: &Connection,
    saadc: &mut Saadc<'_, 2>,
) {
    let debounce = Duration::from_millis(20);
    info!("analog stick service online");
    let mut buf = [0i16; 2];
    saadc.calibrate().await;
    saadc.sample(&mut buf).await;
    // full range around 1870, so divide by 370 to get a range of -5 to 5
    let divider = 370;
    let mut x_axis = Axis::new(buf[0], divider);
    let mut y_axis = Axis::new(buf[1], divider);
    loop {
        // read adc values for x and y, and if they have changed by a certain amount, notify
        // we are reducing the number of analogue stick levels to a range of -5 to 5
        saadc.sample(&mut buf).await;
        if let Some(x) = x_axis.changed(buf[0]) {
            server.stick.x_notify(connection, &x).ok();
        }
        if let Some(y) = y_axis.changed(buf[1]) {
            server.stick.y_notify(connection, &y).ok();
        }
        Timer::after(debounce).await;
    }
}
