//! Read-only CAT probes. Every probe here only *reads* information from the
//! radio (identity or frequency). None of them changes a setting or keys the
//! transmitter, and they are only ever sent when the user explicitly asks.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatProtocol {
    /// `ID;` — Kenwood, Elecraft and current Yaesu radios.
    KenwoodId,
    /// `FA;` — read VFO A frequency (Kenwood/Yaesu/Elecraft ASCII CAT).
    KenwoodFrequency,
    /// CI-V "read transceiver ID" (command 19 00).
    IcomId,
    /// CI-V "read operating frequency" (command 03).
    IcomFrequency,
    /// Legacy 5-byte Yaesu CAT (FT-817/857/897): read frequency and mode.
    YaesuLegacyFrequency,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatProbeInfo {
    pub protocol: CatProtocol,
    pub label: &'static str,
    pub description: &'static str,
    pub default_baud: u32,
    pub default_stop_bits: u8,
}

pub const PROBES: &[CatProbeInfo] = &[
    CatProbeInfo {
        protocol: CatProtocol::KenwoodId,
        label: "ASCII CAT: read radio ID (ID;)",
        description: "Kenwood-style text CAT, used by Kenwood, Elecraft, current Yaesu radios, FlexRadio, QRP Labs, (tr)uSDX, Lab599 and many others.",
        default_baud: 0,
        default_stop_bits: 1,
    },
    CatProbeInfo {
        protocol: CatProtocol::KenwoodFrequency,
        label: "ASCII CAT: read VFO A frequency (FA;)",
        description: "Kenwood-style text CAT frequency query.",
        default_baud: 0,
        default_stop_bits: 1,
    },
    CatProbeInfo {
        protocol: CatProtocol::IcomId,
        label: "CI-V: read transceiver ID",
        description: "Icom CI-V protocol, also used by Xiegu and other CI-V compatible radios.",
        default_baud: 0,
        default_stop_bits: 1,
    },
    CatProbeInfo {
        protocol: CatProtocol::IcomFrequency,
        label: "CI-V: read operating frequency",
        description: "Icom CI-V frequency query.",
        default_baud: 0,
        default_stop_bits: 1,
    },
    CatProbeInfo {
        protocol: CatProtocol::YaesuLegacyFrequency,
        label: "Legacy Yaesu 5-byte CAT: read frequency and mode",
        description: "Binary CAT of older Yaesu radios such as the FT-817/818, FT-857 and FT-897.",
        default_baud: 0,
        default_stop_bits: 2,
    },
];

/// Baud rates tried, in order, when the user selects automatic detection.
pub fn auto_baud_rates(protocol: CatProtocol) -> &'static [u32] {
    match protocol {
        CatProtocol::YaesuLegacyFrequency => &[4800, 9600, 38400],
        CatProtocol::IcomId | CatProtocol::IcomFrequency => {
            &[19200, 9600, 115200, 38400, 57600, 4800]
        }
        CatProtocol::KenwoodId | CatProtocol::KenwoodFrequency => {
            &[38400, 9600, 4800, 19200, 57600, 115200]
        }
    }
}

/// Controller address used in CI-V frames sent by the computer.
const CIV_CONTROLLER: u8 = 0xE0;

pub fn request_bytes(protocol: CatProtocol, civ_address: u8) -> Vec<u8> {
    match protocol {
        CatProtocol::KenwoodId => b"ID;".to_vec(),
        CatProtocol::KenwoodFrequency => b"FA;".to_vec(),
        CatProtocol::IcomId => vec![0xFE, 0xFE, civ_address, CIV_CONTROLLER, 0x19, 0x00, 0xFD],
        CatProtocol::IcomFrequency => vec![0xFE, 0xFE, civ_address, CIV_CONTROLLER, 0x03, 0xFD],
        CatProtocol::YaesuLegacyFrequency => vec![0x00, 0x00, 0x00, 0x00, 0x03],
    }
}

/// Whether enough of a response has arrived to stop reading early.
pub fn response_complete(protocol: CatProtocol, received: &[u8], sent: &[u8]) -> bool {
    match protocol {
        CatProtocol::KenwoodId | CatProtocol::KenwoodFrequency => received.contains(&b';'),
        CatProtocol::IcomId | CatProtocol::IcomFrequency => civ_frames(received)
            .iter()
            .any(|f| f.as_slice() != sent && f.get(2) == Some(&CIV_CONTROLLER)),
        CatProtocol::YaesuLegacyFrequency => received.len() >= 5,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatInterpretation {
    /// A well-formed reply of the expected protocol was received.
    pub recognized: bool,
    pub summary: String,
}

/// Model registered for an ASCII CAT `ID` reply. Only a selection is listed;
/// unknown IDs are still reported.
fn model_for_id(id: &str) -> Option<&'static str> {
    Some(match id {
        "0310" => "Yaesu FT-950",
        "0251" => "Yaesu FT-2000",
        "0362" => "Yaesu FTDX5000",
        "0460" => "Yaesu FTDX3000",
        "0570" => "Yaesu FT-991/FT-991A",
        "0583" => "Yaesu FTDX1200",
        "0650" => "Yaesu FT-891",
        "0681" => "Yaesu FTDX101D",
        "0682" => "Yaesu FTDX101MP",
        "0761" => "Yaesu FTDX10",
        "0800" => "Yaesu FT-710",
        "017" => "Elecraft K2/K3/KX2/KX3",
        "019" => "Kenwood TS-2000",
        "020" => "Kenwood TS-480",
        "021" => "Kenwood TS-590S",
        "023" => "Kenwood TS-590SG",
        _ => return None,
    })
}

fn icom_model(address: u8) -> Option<&'static str> {
    Some(match address {
        0x5E => "IC-718",
        0x56 => "IC-746",
        0x6E => "IC-756PROIII",
        0x70 => "IC-7000",
        0x76 => "IC-7200",
        0x7C => "IC-9100",
        0x80 => "IC-7410",
        0x88 => "IC-7100",
        0x8E => "IC-7851",
        0x94 => "IC-7300",
        0x96 => "IC-R8600",
        0x98 => "IC-7610",
        0xA2 => "IC-9700",
        0xA4 => "IC-705",
        _ => return None,
    })
}

/// Common default CI-V addresses offered in the UI.
pub const ICOM_ADDRESSES: &[(u8, &str)] = &[
    (0x94, "IC-7300"),
    (0x98, "IC-7610"),
    (0xA2, "IC-9700"),
    (0xA4, "IC-705"),
    (0x88, "IC-7100"),
    (0x8E, "IC-7851"),
    (0x76, "IC-7200"),
    (0x70, "IC-7000"),
];

/// Splits a byte stream into CI-V frames (`FE FE … FD`).
fn civ_frames(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0xFE && bytes[i + 1] == 0xFE {
            let mut start = i;
            while start + 2 < bytes.len() && bytes[start + 2] == 0xFE {
                start += 1; // tolerate extra preamble bytes
            }
            if let Some(end) = bytes[start..].iter().position(|b| *b == 0xFD) {
                frames.push(bytes[start..start + end + 1].to_vec());
                i = start + end + 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    frames
}

fn bcd_digits(byte: u8) -> Option<(u8, u8)> {
    let (hi, lo) = (byte >> 4, byte & 0x0F);
    (hi <= 9 && lo <= 9).then_some((hi, lo))
}

fn format_hz(hz: u64) -> String {
    let mhz = hz / 1_000_000;
    let khz = (hz / 1_000) % 1_000;
    let rest = hz % 1_000;
    format!("{mhz}.{khz:03}.{rest:03} MHz")
}

fn ascii_reply(bytes: &[u8]) -> Option<String> {
    let text: String = bytes
        .iter()
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|b| *b as char)
        .collect();
    (!text.is_empty()).then_some(text)
}

pub fn interpret_response(
    protocol: CatProtocol,
    received: &[u8],
    sent: &[u8],
) -> CatInterpretation {
    let none = |summary: &str| CatInterpretation {
        recognized: false,
        summary: summary.to_string(),
    };
    if received.is_empty() {
        return none(
            "No reply. Check the baud rate, that this is the CAT port, and that CAT is enabled on the radio.",
        );
    }
    match protocol {
        CatProtocol::KenwoodId => {
            let text = ascii_reply(received).unwrap_or_default();
            if let Some(start) = text.find("ID") {
                let reply = &text[start..];
                let id: String = reply[2..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if !id.is_empty() && reply[2 + id.len()..].starts_with(';') {
                    // IDs 019 (TS-2000) and 020 (TS-480) are emulated by many
                    // radios and CAT programs.
                    let model = match (model_for_id(&id), id.as_str()) {
                        (Some(m), "019" | "020") => format!(" ({m} or a radio emulating it)"),
                        (Some(m), _) => format!(" (listed as {m})"),
                        (None, _) => String::new(),
                    };
                    return CatInterpretation {
                        recognized: true,
                        summary: format!("Radio answered with ID {id}{model}."),
                    };
                }
            }
            if text.contains("?;") {
                return none(
                    "The device answered '?;' (command not understood): it speaks an ASCII CAT dialect but not this command.",
                );
            }
            none("A reply was received but it is not a Kenwood/Yaesu ID response.")
        }
        CatProtocol::KenwoodFrequency => {
            let text = ascii_reply(received).unwrap_or_default();
            if let Some(start) = text.find("FA") {
                let digits: String = text[start + 2..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if digits.len() >= 8
                    && let Ok(hz) = digits.parse::<u64>()
                {
                    return CatInterpretation {
                        recognized: true,
                        summary: format!("VFO A is on {}.", format_hz(hz)),
                    };
                }
            }
            none("A reply was received but it is not a frequency response.")
        }
        CatProtocol::IcomId | CatProtocol::IcomFrequency => {
            let frames = civ_frames(received);
            let replies: Vec<&Vec<u8>> = frames
                .iter()
                .filter(|f| f.as_slice() != sent && f.get(2) == Some(&CIV_CONTROLLER))
                .collect();
            let Some(reply) = replies.first() else {
                if frames.iter().any(|f| f.as_slice() == sent) {
                    return none(
                        "Only the echo of the command was received; the radio did not answer. Check the CI-V address and baud rate.",
                    );
                }
                return none(
                    "A reply was received but it contains no CI-V frame addressed to the computer.",
                );
            };
            let from = reply.get(3).copied().unwrap_or_default();
            let body = &reply[4..reply.len() - 1];
            if body == [0xFA] {
                return none("The radio rejected the command (CI-V NG).");
            }
            match (protocol, body) {
                (CatProtocol::IcomId, [0x19, 0x00, id, ..]) => {
                    let model = icom_model(*id)
                        .map(|m| format!(" — default address of the {m}"))
                        .unwrap_or_default();
                    CatInterpretation {
                        recognized: true,
                        summary: format!(
                            "Radio at CI-V address {from:02X}h reports ID {id:02X}h{model}."
                        ),
                    }
                }
                (CatProtocol::IcomFrequency, [0x03, bcd @ ..]) if bcd.len() >= 5 => {
                    let mut hz: u64 = 0;
                    let mut scale: u64 = 1;
                    for byte in &bcd[..5] {
                        let Some((hi, lo)) = bcd_digits(*byte) else {
                            return none("The frequency reply contains invalid BCD digits.");
                        };
                        hz += u64::from(lo) * scale + u64::from(hi) * scale * 10;
                        scale *= 100;
                    }
                    CatInterpretation {
                        recognized: true,
                        summary: format!(
                            "Radio at CI-V address {from:02X}h is on {}.",
                            format_hz(hz)
                        ),
                    }
                }
                _ => CatInterpretation {
                    recognized: true,
                    summary: format!("Radio at CI-V address {from:02X}h answered."),
                },
            }
        }
        CatProtocol::YaesuLegacyFrequency => {
            if received.len() < 5 {
                return none("The reply is too short for a Yaesu frequency response.");
            }
            let mut digits = String::new();
            for byte in &received[..4] {
                let Some((hi, lo)) = bcd_digits(*byte) else {
                    return none("The reply is not a valid Yaesu frequency response.");
                };
                digits.push((b'0' + hi) as char);
                digits.push((b'0' + lo) as char);
            }
            let hz = digits.parse::<u64>().unwrap_or_default() * 10;
            let mode = match received[4] {
                0x00 => "LSB",
                0x01 => "USB",
                0x02 => "CW",
                0x03 => "CW-R",
                0x04 => "AM",
                0x08 => "FM",
                0x0A => "DIG",
                0x0C => "PKT",
                _ => "unknown mode",
            };
            CatInterpretation {
                recognized: hz > 0,
                summary: format!("Radio is on {} ({mode}).", format_hz(hz)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kenwood_style_replies() {
        let sent = request_bytes(CatProtocol::KenwoodId, 0);
        assert_eq!(sent, b"ID;");
        let r = interpret_response(CatProtocol::KenwoodId, b"ID0761;", &sent);
        assert!(r.recognized);
        assert_eq!(
            r.summary,
            "Radio answered with ID 0761 (listed as Yaesu FTDX10)."
        );
        let r = interpret_response(CatProtocol::KenwoodId, b"ID019;", &sent);
        assert!(r.summary.contains("TS-2000 or a radio emulating it"));
        let r = interpret_response(CatProtocol::KenwoodId, b"ID9999;", &sent);
        assert_eq!(r.summary, "Radio answered with ID 9999.");
        assert!(response_complete(CatProtocol::KenwoodId, b"ID0761;", &sent));
        let r = interpret_response(CatProtocol::KenwoodId, b"?;", &sent);
        assert!(!r.recognized);
        let r = interpret_response(CatProtocol::KenwoodFrequency, b"FA014074000;", b"FA;");
        assert_eq!(r.summary, "VFO A is on 14.074.000 MHz.");
        let r = interpret_response(CatProtocol::KenwoodFrequency, b"FA00007074000;", b"FA;");
        assert_eq!(r.summary, "VFO A is on 7.074.000 MHz.");
        assert!(!interpret_response(CatProtocol::KenwoodId, b"", &sent).recognized);
    }

    #[test]
    fn icom_replies_with_echo() {
        let sent = request_bytes(CatProtocol::IcomId, 0x94);
        assert_eq!(sent, vec![0xFE, 0xFE, 0x94, 0xE0, 0x19, 0x00, 0xFD]);
        let mut received = sent.clone(); // USB echo
        received.extend([0xFE, 0xFE, 0xE0, 0x94, 0x19, 0x00, 0x94, 0xFD]);
        assert!(response_complete(CatProtocol::IcomId, &received, &sent));
        let r = interpret_response(CatProtocol::IcomId, &received, &sent);
        assert!(r.recognized);
        assert!(r.summary.contains("IC-7300"));

        assert!(!response_complete(CatProtocol::IcomId, &sent, &sent));
        let r = interpret_response(CatProtocol::IcomId, &sent, &sent);
        assert!(r.summary.starts_with("Only the echo"));

        let sent = request_bytes(CatProtocol::IcomFrequency, 0xA4);
        let reply = [
            0xFE, 0xFE, 0xE0, 0xA4, 0x03, 0x00, 0x40, 0x07, 0x14, 0x00, 0xFD,
        ];
        let r = interpret_response(CatProtocol::IcomFrequency, &reply, &sent);
        assert_eq!(r.summary, "Radio at CI-V address A4h is on 14.074.000 MHz.");

        let ng = [0xFE, 0xFE, 0xE0, 0xA4, 0xFA, 0xFD];
        assert!(
            interpret_response(CatProtocol::IcomFrequency, &ng, &sent)
                .summary
                .contains("NG")
        );
    }

    #[test]
    fn yaesu_legacy_reply() {
        let sent = request_bytes(CatProtocol::YaesuLegacyFrequency, 0);
        let r = interpret_response(
            CatProtocol::YaesuLegacyFrequency,
            &[0x01, 0x40, 0x74, 0x00, 0x01],
            &sent,
        );
        assert!(r.recognized);
        assert_eq!(r.summary, "Radio is on 14.074.000 MHz (USB).");
    }
}
