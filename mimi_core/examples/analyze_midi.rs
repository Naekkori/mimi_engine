use midly::Smf;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "d:/source/mimi_engine/assets/tj_edm_like.mid".to_string());
    let bytes = std::fs::read(&path).expect("read failed");
    let smf = Smf::parse(&bytes).expect("parse failed");

    println!("=== {} 분석 ===", path);
    println!("포맷: {:?}", smf.header.format);
    println!("트랙 수: {}", smf.tracks.len());
    println!("PPQ: {:?}", smf.header.timing);

    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u32;
        println!("\n=== 트랙 {} ===", ti);
        for event in track.iter() {
            tick += event.delta.as_int();
            match &event.kind {
                midly::TrackEventKind::Midi { channel, message } => {
                    let ch = u8::from(*channel);
                    match message {
                        midly::MidiMessage::NoteOn { key, vel } => {
                            println!("T{} tick={} ch={} NoteOn key={} vel={}", ti, tick, ch+1, key.as_int(), vel.as_int());
                        }
                        midly::MidiMessage::NoteOff { key, vel } => {
                            println!("T{} tick={} ch={} NoteOff key={} vel={}", ti, tick, ch+1, key.as_int(), vel.as_int());
                        }
                        midly::MidiMessage::Controller { controller, value } => {
                            println!("T{} tick={} ch={} CC#{}={}", ti, tick, ch+1, controller.as_int(), value.as_int());
                        }
                        midly::MidiMessage::ProgramChange { program } => {
                            println!("T{} tick={} ch={} PC={}", ti, tick, ch+1, program.as_int());
                        }
                        midly::MidiMessage::PitchBend { bend } => {
                            println!("T{} tick={} ch={} PitchBend={:?}", ti, tick, ch+1, bend);
                        }
                        _ => {}
                    }
                }
                midly::TrackEventKind::Meta(meta) => {
                    match meta {
                        midly::MetaMessage::TrackName(name) => {
                            let name_str = String::from_utf8_lossy(name);
                            println!("T{} tick={} TrackName: {}", ti, tick, name_str);
                        }
                        midly::MetaMessage::Text(text) => {
                            let text_str = String::from_utf8_lossy(text);
                            println!("T{} tick={} Text: {}", ti, tick, text_str);
                        }
                        midly::MetaMessage::Marker(marker) => {
                            let marker_str = String::from_utf8_lossy(marker);
                            println!("T{} tick={} *** MARKER: {} ***", ti, tick, marker_str);
                        }
                        midly::MetaMessage::CuePoint(cue) => {
                            let cue_str = String::from_utf8_lossy(cue);
                            println!("T{} tick={} CuePoint: {}", ti, tick, cue_str);
                        }
                        midly::MetaMessage::Tempo(tempo) => {
                            println!("T{} tick={} Tempo: {} µs/beat", ti, tick, tempo.as_int());
                        }
                        midly::MetaMessage::TimeSignature(n, d, c, b) => {
                            println!("T{} tick={} TimeSignature: {}/{}, clocks={}, {}", ti, tick, n, 1 << d, c, b);
                        }
                        midly::MetaMessage::KeySignature(sharps, is_minor) => {
                            println!("T{} tick={} KeySignature: {} sharps, minor={}", ti, tick, sharps, is_minor);
                        }
                        midly::MetaMessage::EndOfTrack => {
                            println!("T{} tick={} EndOfTrack", ti, tick);
                        }
                        _ => {}
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

    // 마커를 기준으로 패턴 분석
    println!("\n=== 패턴 분석 ===");
    let mut markers = Vec::new();
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u32;
        for event in track.iter() {
            tick += event.delta.as_int();
            if let midly::TrackEventKind::Meta(meta) = &event.kind {
                match meta {
                    midly::MetaMessage::Marker(marker) => {
                        let marker_str = String::from_utf8_lossy(marker);
                        markers.push((ti, tick, marker_str.to_string()));
                    }
                    midly::MetaMessage::Text(text) => {
                        let text_str = String::from_utf8_lossy(text);
                        if text_str.contains("패턴") || text_str.contains("리듬") || text_str.contains("EDM") {
                            markers.push((ti, tick, format!("TEXT: {}", text_str)));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if markers.is_empty() {
        println!("마커를 찾을 수 없습니다. 트랙 이름이나 텍스트 이벤트를 확인합니다.");
        for (ti, track) in smf.tracks.iter().enumerate() {
            let mut tick = 0u32;
            for event in track.iter() {
                tick += event.delta.as_int();
                if let midly::TrackEventKind::Meta(meta) = &event.kind {
                    if let midly::MetaMessage::TrackName(name) = meta {
                        let name_str = String::from_utf8_lossy(name);
                        if name_str.contains("패턴") || name_str.contains("리듬") || name_str.contains("EDM") {
                            println!("T{} tick={} TrackName에 패턴 키워드 발견: {}", ti, tick, name_str);
                        }
                    }
                }
            }
        }
    } else {
        println!("발견된 마커:");
        for (ti, tick, marker) in &markers {
            println!("  T{} tick={} marker: {}", ti, tick, marker);
        }
    }
}