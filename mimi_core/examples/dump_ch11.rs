use midly::Smf;

fn main() {
    let bytes = std::fs::read("assets/HAPPY.mid").expect("read failed");
    let smf = Smf::parse(&bytes).expect("parse failed");

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
                            midly::MidiMessage::NoteOn { key, vel } if tick < 500 => {
                                println!("T{} tick={} ch=11 NoteOn key={} vel={}", ti, tick, key.as_int(), vel.as_int());
                            }
                            _ => {}
                        }
                    }
                }
                midly::TrackEventKind::SysEx(data) => {
                    let hex: Vec<String> = data.iter().map(|b| format!("{:02X}", b)).collect();
                    println!("T{} tick={} SysEx: {}", ti, tick, hex.join(" "));
                }
                _ => {}
            }
        }
    }
}
