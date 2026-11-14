//! End-to-end checks for the real RGB1W image in the VM.

mod common;

use pioc_programs::ws2812::{ColorOrder, Config, OutputPin, Rgb8, Storage, Ws2812};
use pioc_programs::PinMask;
use pioc_programs::{CommandPort, Error, Firmware};

use common::{block_on, expected_bits, Operation, VmPort};

const RGB1W_48M: &[u16] = pioc_proc::pioc_file_canonical!("../examples/canonical/rgb1w.pio");

const FIRMWARE: Firmware = Firmware {
    words: RGB1W_48M,
    sfr_bit_cycles: 56,
};

fn twelve_leds() -> [Rgb8; 12] {
    std::array::from_fn(|index| Rgb8::new(index as u8 * 3, 255 - index as u8 * 3, index as u8))
}

/// Split the measured high times into zero and one bits.
fn bit_threshold(pulses: &[u64]) -> u64 {
    let minimum = *pulses.iter().min().expect("the firmware never drove IO0");
    let maximum = *pulses.iter().max().expect("the firmware never drove IO0");
    assert!(
        minimum < maximum,
        "payload must contain both zero and one bits, got {minimum} cycles for every bit"
    );
    (minimum + maximum) / 2
}

#[test]
fn one_led_over_the_sfr_path_matches_the_rgb_bytes() {
    let mut driver = Ws2812::new(VmPort::new(RGB1W_48M), FIRMWARE, Config::default());
    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert!(
        !port
            .operations
            .iter()
            .any(|operation| matches!(operation, Operation::WriteRam(..))),
        "a three-byte frame on IO0 must stay on the SFR path: {:?}",
        port.operations
    );
    let pulses = port.high_pulses();
    let threshold = bit_threshold(&pulses);
    assert_eq!(port.bits(threshold), expected_bits(&[0x00, 0xff, 0x00]));
}

#[test]
fn twelve_leds_land_in_program_ram_and_stream_from_there() {
    let colors = twelve_leds();
    let payload = colors
        .iter()
        .flat_map(|color| ColorOrder::Grb.pack(*color))
        .collect::<Vec<_>>();

    let mut driver = Ws2812::new(VmPort::new(RGB1W_48M), FIRMWARE, Config::default());
    block_on(driver.write_rgb(&colors, ColorOrder::Grb)).unwrap();
    let port = driver.release();

    // One RAM write per colour, starting at 0x400.
    let ram_writes = port
        .operations
        .iter()
        .filter_map(|operation| match operation {
            Operation::WriteRam(offset, data) => Some((*offset, data.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ram_writes.len(), colors.len());
    assert_eq!(ram_writes[0], (0x400, payload[..3].to_vec()));
    // Byte offset 0x400 is program word 0x200.
    let program = port.machine.program();
    for (index, pair) in payload.chunks(2).enumerate() {
        let low = u16::from(pair[0]);
        let high = pair.get(1).map_or(0, |byte| u16::from(*byte) << 8);
        assert_eq!(
            program[0x200 + index],
            low | high,
            "program RAM word {index} does not hold the payload"
        );
    }
    let pulses = port.high_pulses();
    let threshold = bit_threshold(&pulses);
    assert_eq!(port.bits(threshold), expected_bits(&payload));
}

#[test]
fn alternating_frame_lengths_stay_on_the_program_ram_path() {
    let colors = twelve_leds();
    let one = Rgb8::new(0x11, 0x22, 0x33);
    let mut driver = Ws2812::new(VmPort::new(RGB1W_48M), FIRMWARE, Config::default());

    block_on(driver.write_rgb(&colors, ColorOrder::Grb)).unwrap();
    block_on(driver.write_rgb(&[one], ColorOrder::Grb)).unwrap();
    block_on(driver.write_rgb(&colors, ColorOrder::Grb)).unwrap();

    // Switching back after the image tail was overwritten must fail.
    driver.set_config(Config {
        storage: Storage::Sfr,
        ..Config::default()
    });
    assert_eq!(
        block_on(driver.write_rgb(&[one], ColorOrder::Grb)).unwrap_err(),
        Error::SfrUnavailable
    );

    let port = driver.release();
    assert_eq!(
        port.operations
            .iter()
            .filter(|operation| matches!(operation, Operation::WriteRam(..)))
            .count(),
        // Twelve colours, then one, then twelve again.
        25
    );
    assert!(!port.operations.contains(&Operation::Command(3)));

    let mut expected = Vec::new();
    for frame in [&colors[..], &[one][..], &colors[..]] {
        for color in frame {
            expected.extend(expected_bits(&ColorOrder::Grb.pack(*color)));
        }
    }
    let pulses = port.high_pulses();
    let threshold = bit_threshold(&pulses);
    assert_eq!(port.bits(threshold), expected);
}

#[test]
fn io1_frames_claim_only_io1_and_use_program_ram() {
    let config = Config {
        output: OutputPin::Io1,
        ..Config::default()
    };
    let mut driver = Ws2812::new(VmPort::new(RGB1W_48M), FIRMWARE, config);
    block_on(driver.write_rgb(&[Rgb8::new(255, 0, 0)], ColorOrder::Grb)).unwrap();
    let port = driver.release();

    assert!(port
        .operations
        .contains(&Operation::WriteRam(0x400, vec![0x00, 0xff, 0x00])));
    assert!(port.operations.contains(&Operation::WriteData(2, vec![1])));
    assert!(port.operations.contains(&Operation::Reset(PinMask::IO1)));
    assert!(port.operations.contains(&Operation::Run(PinMask::IO1)));
}

#[test]
fn a_request_raised_before_the_wait_is_not_missed() {
    let mut port = VmPort::new(RGB1W_48M);
    port.reset_halted(PinMask::IO0).unwrap();
    port.run(PinMask::IO0).unwrap();
    port.write_data(0, &[0x00, 0xff, 0x00]).unwrap();
    port.start_command(3).unwrap();

    // Finish the firmware before the driver starts waiting.
    let mut steps = 0;
    while port.machine.register(0x1c).unwrap() & 0x80 == 0 {
        port.machine.step_or_idle().unwrap();
        steps += 1;
        assert!(steps < 100_000, "firmware never raised the interrupt");
    }
    let cycles = port.machine.cycles;

    assert_eq!(block_on(port.wait_for_interrupt()).unwrap(), 0);
    assert_eq!(
        port.machine.cycles, cycles,
        "an already-raised request must be serviced without running the firmware"
    );
    assert_eq!(
        port.machine.register(0x1c).unwrap() & 0x80,
        0,
        "reading the status must acknowledge the request"
    );
}
