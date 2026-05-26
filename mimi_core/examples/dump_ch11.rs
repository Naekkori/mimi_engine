use midly::Smf;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "assets/test.mid".to_string());
    let bytes = std::fs::read(&path).expect("read failed");
    let smf = Smf::parse(&bytes).expect("parse failed");

    println!("=== {} SysEx & Ch11 dump ===", path);
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u32;
        for event in track.iter() {
            tick += event.delta.as_int();
            match &event.kind {
                midly::TrackEventKind::Midi { channel, message } => {
                    let ch = u8::from(*channel);
                    if ch == 10 {
                        match message {
                            midly::MidiMessage::Controller { controller, value } => {
                                println!("T{} tick={} ch=11 CC#{}={}", ti, tick, controller.as_int(), value.as_int());
                            }
                            midly::MidiMessage::ProgramChange { program } => {
                                println!("T{} tick={} ch=11 PC={}", ti, tick, program.as_int());
                            }
                            midly::MidiMessage::NoteOn { key, vel } if tick < 1000 => {
                                println!("T{} tick={} ch=11 NoteOn key={} vel={}", ti, tick, key.as_int(), vel.as_int());
                            }
                            _ => {}
                        }
                    }
                }
                midly::TrackEventKind::SysEx(data) => {
                    let hex: Vec<String> = data.iter().map(|b| format!("{:02X}", b)).collect();
                    println!("T{} tick={} SysEx[len={}]: {}", ti, tick, data.len(), hex.join(" "));

                    // GS Rhythm Part Assign 감지
                    if data.len() >= 9
                        && data[0] == 0x41
                        && data[2] == 0x42
                        && data[3] == 0x12
                        && data[4] == 0x40
                        && (data[5] & 0xF0) == 0x10
                        && data[6] == 0x15
                    {
                        let part = data[5] & 0x0F;
                        let ch = match part {
                            0 => 9,
                            p if p >= 1 && p <= 9 => p - 1,
                            p if p >= 10 && p <= 15 => p,
                            _ => 9,
                        };
                        let is_drum = data[7] == 1 || data[7] == 2;
                        println!("  -> GS DrumPart: part={} -> ch={} (1-based {}) is_drum={}", part, ch, ch+1, is_drum);
                    }

                    // XG Part Mode 감지
                    if data.len() >= 7
                        && data[0] == 0x43
                        && data[2] == 0x4C
                        && data[3] == 0x08
                        && (data[4] & 0xF0) == 0x10
                        && data[5] == 0x0E
                    {
                        let part = data[4] & 0x0F;
                        let is_drum = data[6] >= 1;
                        println!("  -> XG DrumPart: part={} (1-based {}) is_drum={}", part, part+1, is_drum);
                    }

                    // GS Reset 감지
                    if data.len() >= 7
                        && data[0] == 0x41 && data[2] == 0x42
                        && data[3] == 0x12 && data[4] == 0x40
                        && data[5] == 0x00 && data[6] == 0x7F
                    {
                        println!("  -> GS RESET");
                    }

                    // GM Reset 감지
                    if data.len() >= 4
                        && data[0] == 0x7E && data[2] == 0x09
                    {
                        println!("  -> GM RESET");
                    }

                    // XG ON 감지
                    if data.len() >= 7
                        && data[0] == 0x43 && data[2] == 0x4C
                        && data[3] == 0x00 && data[4] == 0x00
                        && data[5] == 0x7E && data[6] == 0x00
                    {
                        println!("  -> XG ON");
                    }
                }
                _ => {}
            }
        }
    }
}
