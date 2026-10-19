use mchose_battery::hid_layout::parse_descriptor;

fn hex(bytes: &str) -> Vec<u8> {
    bytes
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).expect("valid hex"))
        .collect()
}

#[test]
fn parses_a5_v3_descriptor() {
    // /sys/class/hidraw/hidraw5/device/report_descriptor
    let descriptor = hex("06 0b ff 09 c4 a1 01 15 00 26 ff 00 75 08 95 3c
         85 2a 09 e0 81 02 09 e1 91 02 09 e2 b2 02 01 85
         2b 09 e5 b2 02 01 17 00 00 00 80 27 ff ff ff 7f
         75 20 95 04 85 2c 19 e6 29 e9 81 02 85 2d 19 8a
         29 8d 81 02 19 8e 29 91 91 02 c0 06 01 ff 09 01
         a1 01 85 11 19 00 29 ff 15 00 26 ff 00 75 08 95
         14 09 20 91 02 09 21 81 00 09 22 b2 02 01 85 4d
         19 00 29 ff 15 00 26 ff 00 75 08 95 3f 09 23 91
         02 09 24 81 00 09 25 b2 02 01 85 13 19 00 29 ff
         15 00 26 ff 00 75 08 95 14 91 02 09 27 81 00 09
         28 b2 02 01 c0");
    let layout = parse_descriptor(&descriptor);

    assert_eq!(layout.feature_size(0x11), Some(21));
    assert_eq!(layout.output_size(0x11), Some(21));
    assert_eq!(layout.feature_size(0x13), Some(21));
    assert_eq!(layout.output_size(0x13), Some(21));
    assert_eq!(layout.feature_size(0x4d), Some(64));
    assert_eq!(layout.feature_size(0x2a), Some(61));
}

#[test]
fn receiver_0x13_is_input_output_only() {
    // /sys/class/hidraw/hidraw2/device/report_descriptor (first collection)
    let descriptor = hex("06 02 ff 09 02 a1 01 85 13 15 00 26 ff 00 75 08
         95 13 09 02 81 00 09 02 91 00 c0");
    let layout = parse_descriptor(&descriptor);

    assert_eq!(layout.input_size(0x13), Some(20));
    assert_eq!(layout.output_size(0x13), Some(20));
    assert_eq!(layout.feature_size(0x13), None);
}
