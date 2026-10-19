use mchose_battery::icon;
use mchose_battery::protocol::newmouse::{
    build_read_info_packet, parse_notification, parse_read_info,
};
use mchose_battery::protocol::{
    parse_a5, parse_a7, parse_gmouse, parse_hub, parse_native11, ReportKind,
};
use mchose_battery::types::{ChargeState, Connection};

fn native_report(vid: u16, pid: u16, flags: u8, battery: u8, charge: u8) -> Vec<u8> {
    let mut payload = [0u8; 20];
    payload[0] = 0x06;
    payload[1..3].copy_from_slice(&vid.to_le_bytes());
    payload[3..5].copy_from_slice(&pid.to_le_bytes());
    payload[9] = flags;
    payload[10] = battery;
    payload[11] = charge;

    let mut wire = vec![0x11];
    wire.extend(payload.iter().map(|byte| byte ^ 0xff));
    wire
}

#[test]
fn native11_decodes_wireless_battery() {
    let report = native_report(0x3837, 0x1018, 0x01, 85, 1);
    let parsed = parse_native11(&report).expect("valid native report");
    assert_eq!(parsed.percent, 85);
    assert_eq!(parsed.charge, ChargeState::Charging);
    assert_eq!(parsed.connection, Connection::Wireless24G);
    assert_eq!(parsed.mouse_vid, Some(0x3837));
    assert_eq!(parsed.mouse_pid, Some(0x1018));
}

#[test]
fn native11_decodes_wired_and_full() {
    let report = native_report(0x3837, 0x1018, 0x00, 100, 2);
    let parsed = parse_native11(&report).expect("valid native report");
    assert_eq!(parsed.percent, 100);
    assert_eq!(parsed.charge, ChargeState::Full);
    assert_eq!(parsed.connection, Connection::Wired);
}

#[test]
fn native11_rejects_sleep_and_power_off() {
    let sleep = native_report(0x3837, 0x1018, 0x06, 85, 0);
    assert!(parse_native11(&sleep).is_err());

    let off = native_report(0x0000, 0x0000, 0x07, 85, 0);
    assert!(parse_native11(&off).is_err());
}

#[test]
fn native11_rejects_impossible_percentage() {
    let report = native_report(0x3837, 0x1018, 0x01, 101, 1);
    assert!(parse_native11(&report).is_err());
}

#[test]
fn hub13_decodes_wireless_ilt_packet() {
    let report = [0x13, 10, 0x00, 0x00, 0x00, 0x05, 85, 0x10];
    let parsed = parse_hub(&report, ReportKind::Wireless13).expect("valid hub packet");
    assert_eq!(parsed.percent, 85);
    assert_eq!(parsed.charge, ChargeState::Charging);
    assert_eq!(parsed.connection, Connection::Wireless24G);
}

#[test]
fn hub06_decodes_wired_bbe_packet() {
    let report = [0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 77, 0x00];
    let parsed = parse_hub(&report, ReportKind::Wired06).expect("valid hub packet");
    assert_eq!(parsed.percent, 77);
    assert_eq!(parsed.charge, ChargeState::NotCharging);
    assert_eq!(parsed.connection, Connection::Wired);
}

#[test]
fn newmouse_builds_0900_query() {
    let packet = build_read_info_packet(64);
    assert_eq!(packet.len(), 64);
    assert_eq!(
        &packet[..9],
        &[0x4d, 0x01, 0x01, 0x00, 0x00, 0x09, 0x00, 0x00, 0x08]
    );
}

#[test]
fn newmouse_decodes_read_info_response() {
    // Response from the official SDK layout: report ID 0x4D, command 0x0900.
    let mut packet = [0u8; 64];
    packet[0] = 0x4d;
    packet[1] = 0x01;
    packet[2] = 0x00;
    packet[3] = 0x00;
    packet[4] = 0x00;
    packet[5] = 0x09;
    packet[6] = 0x00;
    packet[7] = 0x00;
    packet[8] = 0x37; // vid low
    packet[9] = 0x38; // vid high
    packet[10] = 0x18; // pid low
    packet[11] = 0x10; // pid high
    packet[12] = 0x03;
    packet[13] = 0x00;
    packet[14] = 0x00;
    packet[15] = 0x00;
    packet[16] = 0x00;
    packet[17] = 0x00;
    packet[18] = 0x00; // connect status
    packet[19] = 0x01; // charging
    packet[20] = 0x64; // 100%
    packet[21] = 0x00; // game mode

    let parsed = parse_read_info(&packet).expect("valid 0x0900 response");
    assert_eq!(parsed.percent, 100);
    assert_eq!(parsed.charge, ChargeState::Charging);
    assert_eq!(parsed.mouse_vid, 0x3837);
    assert_eq!(parsed.mouse_pid, 0x1018);
    assert_eq!(parsed.connect_status, 0);
}

#[test]
fn newmouse_rejects_error_response() {
    // Real idle/dock response: 4d 01 01 01 00 09 00 00 ff ...
    let mut packet = [0u8; 64];
    packet[0] = 0x4d;
    packet[1] = 0x01;
    packet[2] = 0x01;
    packet[3] = 0x01;
    packet[4] = 0x00;
    packet[5] = 0x09;
    packet[6] = 0x00;
    packet[7] = 0x00;
    packet[8] = 0xff;
    assert!(parse_read_info(&packet).is_err());
}

#[test]
fn newmouse_decodes_0600_battery_notification() {
    let mut packet = [0u8; 64];
    packet[0] = 0x4d;
    packet[1] = 0x01;
    packet[2] = 0x00;
    packet[3] = 0x0c;
    packet[4] = 0x00;
    packet[5] = 0x06;
    packet[6] = 0x00;
    packet[7] = 0x00;
    packet[8] = 0x02; // battery flag
    packet[9] = 0x02; // charge flag
    packet[10] = 0x00; // dpi
    packet[11] = 85; // battery
    packet[20] = 0x01; // charge status

    let (percent, charge) = parse_notification(&packet).expect("valid notification");
    assert_eq!(percent, 85);
    assert_eq!(charge, ChargeState::Charging);
}

#[test]
fn newmouse_notification_with_zero_battery_is_ignored() {
    // Real notification captured while the mouse was idle on the dock.
    let mut packet = [0u8; 64];
    packet[0] = 0x4d;
    packet[1] = 0x01;
    packet[2] = 0x00;
    packet[3] = 0x0c;
    packet[4] = 0x00;
    packet[5] = 0x06;
    packet[6] = 0x00;
    packet[7] = 0x00;
    packet[8] = 0x02;
    packet[9] = 0x01;
    assert!(parse_notification(&packet).is_err());
}

#[test]
fn hub13_decodes_4a_checksum_packet() {
    // Real response from the user's 41e4:2004 receiver:
    // 13 4a 01 00 02 64 01 00 ... c5
    let packet = [
        0x13, 0x4a, 0x01, 0x00, 0x02, 0x64, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0xc5,
    ];
    let parsed = parse_hub(&packet, ReportKind::Wireless13).expect("valid 0x4a packet");
    assert_eq!(parsed.percent, 100);
    assert_eq!(parsed.charge, ChargeState::Charging);
}

#[test]
fn hub13_rejects_unrelated_0x4d_input() {
    // The A5 V3 wired interface echoes the 0x11 command in a 0x4d input report;
    // it must not be parsed as a 0% battery reading.
    let packet = [
        0x4d, 0xff, 0x00, 0x01, 0xff, 0xff, 0xff, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x6c,
    ];
    assert!(parse_hub(&packet, ReportKind::Wireless13).is_err());
}

#[test]
fn hub_rejects_invalid_percentage() {
    let report = [0x13, 10, 0x00, 0x00, 0x00, 0x05, 101, 0x00];
    assert!(parse_hub(&report, ReportKind::Wireless13).is_err());
}

#[test]
fn a5_decodes_battery() {
    let mut report = [0u8; 65];
    report[7] = 1;
    report[8] = 42;
    assert_eq!(parse_a5(&report).unwrap(), (42, 1));
}

#[test]
fn a5_rejects_invalid_percentage() {
    let mut report = [0u8; 65];
    report[8] = 120;
    assert!(parse_a5(&report).is_err());
}

#[test]
fn a7_decodes_legacy_payload() {
    let mut raw = [0u8; 21];
    raw[0] = 0x11 ^ 0xff;
    raw[1] = 0x06;
    raw[11] = 66;
    raw[12] = 2;
    let wire: Vec<u8> = raw.iter().map(|byte| byte ^ 0xff).collect();
    assert_eq!(parse_a7(&wire).unwrap(), (66, 2));
}

#[test]
fn gmouse_decodes_aa30_payload() {
    let report = [0xaa, 0x30, 0, 0, 0, 0, 0, 0, 66, 1];
    assert_eq!(parse_gmouse(&report).unwrap(), (66, 1));

    let shifted = [0x00, 0xaa, 0x30, 0, 0, 0, 0, 0, 0, 55, 0];
    assert_eq!(parse_gmouse(&shifted).unwrap(), (55, 0));
}

#[test]
fn icon_is_rendered_at_expected_size() {
    let charging = icon::render_rgba(Some(85), ChargeState::Charging, true, false);
    assert_eq!(charging.len(), (icon::SIZE * icon::SIZE * 4) as usize);
    assert!(charging.chunks(4).any(|pixel| pixel[3] != 0));

    let unknown = icon::render_rgba(None, ChargeState::Unknown, false, false);
    assert_eq!(unknown.len(), (icon::SIZE * icon::SIZE * 4) as usize);
    assert!(unknown.chunks(4).any(|pixel| pixel[3] != 0));
}
