#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

use core::cell::RefCell;
use cortex_m::asm;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use critical_section::Mutex;
use hal::{
    adc,
    pac::{self, interrupt},
    prelude::*,
    timer,
};
use stm32f3xx_hal as hal;

static TIMER: Mutex<RefCell<Option<timer::Timer<pac::TIM2>>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    hprintln!("Hello, World");
    let dp = pac::Peripherals::take().unwrap();
    let mut rcc = dp.RCC.constrain();
    let mut flash = dp.FLASH.constrain();
    let clocks = rcc.cfgr.freeze(&mut flash.acr);

    dp.DBGMCU.cr.modify(|_, w| {
        w.dbg_sleep().set_bit();
        w.dbg_standby().set_bit();
        w.dbg_stop().set_bit()
    });

    let mut common_adc = adc::CommonAdc::new(dp.ADC1_2, &clocks, &mut rcc.ahb);
    let mut adcs = (dp.ADC1, dp.ADC2);
    let mut adc =
        adc::Adc::new(adcs.0, adc::config::Config::default(), &clocks, &common_adc).into_oneshot();

    let mut gpioa = dp.GPIOA.split(&mut rcc.ahb);
    let mut analog_pin = gpioa.pa0.into_analog(&mut gpioa.moder, &mut gpioa.pupdr);

    let mut timer = timer::Timer::new(dp.TIM2, clocks, &mut rcc.apb1);

    unsafe {
        cortex_m::peripheral::NVIC::unmask(timer.interrupt());
    }
    timer.enable_interrupt(timer::Event::Update);
    // Start a timer which fires regularly to wake up from `asm::wfi`
    timer.start(500.milliseconds());
    // Put the timer in the global context.
    critical_section::with(|cs| {
        TIMER.borrow(cs).replace(Some(timer));
    });

    loop {
        let adc_data: u16 = adc.read(&mut analog_pin).unwrap();
        hprintln!("{} mV", to_volts(adc_data));
        asm::delay(8_000_000);
    }
}

fn to_volts(analog: u16) -> u16 {
    let max_adc: u16 = 2u16.pow(12) - 1;
    let reference_mv: u16 = 3300;
    let result = (analog as u32 * reference_mv as u32 ) / max_adc as u32;
    result as u16
}

#[interrupt]
fn TIM2() {
    // Just handle the pending interrupt event.
    critical_section::with(|cs| {
        TIMER
            // Unlock resource for use in critical section
            .borrow(cs)
            // Get a mutable reference from the RefCell
            .borrow_mut()
            // Make the inner Option<T> -> Option<&mut T>
            .as_mut()
            // Unwrap the option, we know, that it has Some()!
            .unwrap()
            // Finally operate on the timer itself.
            .clear_event(timer::Event::Update);
    })
}
