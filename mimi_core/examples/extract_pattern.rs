use midly::Smf;
use std::collections::HashMap;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "d:/source/mimi_engine/assets/tj_edm_like.mid".to_string());
    let bytes = std::fs::read(&path).expect("read failed");
    let smf = Smf::parse(&bytes).expect("parse failed");

    // 마커 수집
    let mut markers = Vec::new();
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u32;
        for event in track.iter() {
            tick += event.delta.as_int();
            if let midly::TrackEventKind::Meta(meta) = &event.kind {
                if let midly::MetaMessage::Marker(marker) = meta {
                    let marker_str = String::from_utf8_lossy(marker);
                    markers.push((ti, tick, marker_str.to_string()));
                }
            }
        }
    }

    // 패턴 구간 정의
    let mut pattern_ranges = Vec::new();
    for i in 0..markers.len() {
        let (_, start_tick, name) = &markers[i];
        let end_tick = if i + 1 < markers.len() {
            markers[i + 1].1
        } else {
            // 마지막 마커부터 트랙 끝까지
            5760 // MIDI 파일에서 확인한 끝 틱
        };
        if name.contains("Pattern") {
            pattern_ranges.push((*start_tick, end_tick, name.clone()));
        }
    }

    println!("=== 패턴 구간 ===");
    for (start, end, name) in &pattern_ranges {
        println!("{}: {} ~ {} (길이 {})", name, start, end, end - start);
    }

    // 각 패턴별로 노트 추출
    for (pattern_idx, (start_tick, end_tick, pattern_name)) in pattern_ranges.iter().enumerate() {
        println!("\n=== {} 노트 추출 ===", pattern_name);

        // 트랙별로 노트 수집
        let mut track_notes: HashMap<usize, Vec<(u32, u8, u8, u8)>> = HashMap::new(); // (tick, note, vel, channel)

        for (ti, track) in smf.tracks.iter().enumerate() {
            let mut tick = 0u32;
            let mut notes_in_range = Vec::new();

            for event in track.iter() {
                tick += event.delta.as_int();
                if tick >= *start_tick && tick < *end_tick {
                    if let midly::TrackEventKind::Midi { channel, message } = &event.kind {
                        if let midly::MidiMessage::NoteOn { key, vel } = message {
                            if vel.as_int() > 0 {
                                let ch = u8::from(*channel);
                                notes_in_range.push((tick - start_tick, key.as_int(), vel.as_int(), ch));
                            }
                        }
                    }
                }
            }

            if !notes_in_range.is_empty() {
                track_notes.insert(ti, notes_in_range);
            }
        }

        // 트랙별 분류
        let mut drum_notes = Vec::new();
        let mut bass_notes = Vec::new();
        let mut accompaniment_notes = Vec::new();

        for (ti, notes) in &track_notes {
            // 트랙 이름이나 채널로 분류
            let is_drum = notes.iter().any(|(_, _, _, ch)| *ch == 9); // 채널 10 (0-based 9)
            let is_bass = notes.iter().any(|(_, note, _, _)| *note < 48); // 낮은 음역대

            if is_drum {
                drum_notes.extend(notes.clone());
            } else if is_bass {
                bass_notes.extend(notes.clone());
            } else {
                accompaniment_notes.extend(notes.clone());
            }
        }

        // 드럼 패턴 출력
        if !drum_notes.is_empty() {
            println!("드럼 패턴:");
            drum_notes.sort_by_key(|(tick, _, _, _)| *tick);
            for (tick, note, vel, ch) in &drum_notes {
                println!("  tick={}, note={}, vel={}, ch={}", tick, note, vel, ch);
            }
        }

        // 베이스 패턴 출력
        if !bass_notes.is_empty() {
            println!("베이스 패턴:");
            bass_notes.sort_by_key(|(tick, _, _, _)| *tick);
            for (tick, note, vel, ch) in &bass_notes {
                println!("  tick={}, note={}, vel={}, ch={}", tick, note, vel, ch);
            }
        }

        // 반주 패턴 출력
        if !accompaniment_notes.is_empty() {
            println!("반주 패턴:");
            accompaniment_notes.sort_by_key(|(tick, _, _, _)| *tick);
            for (tick, note, vel, ch) in &accompaniment_notes {
                println!("  tick={}, note={}, vel={}, ch={}", tick, note, vel, ch);
            }
        }
    }

    // rhythm_engine.rs에 추가할 코드 생성
    println!("\n=== rhythm_engine.rs에 추가할 코드 ===");
    for (pattern_idx, (start_tick, end_tick, pattern_name)) in pattern_ranges.iter().enumerate() {
        let pattern_variants = ["Edm2", "Edm3"]; // 두 가지 패턴
        let variant_name = if pattern_idx < pattern_variants.len() {
            pattern_variants[pattern_idx]
        } else {
            "EdmUnknown"
        };

        println!("// {} 패턴 ({} ~ {})", pattern_name, start_tick, end_tick);
        println!("let mut edm{}_tracks = Vec::new();", pattern_idx + 2);

        // 드럼 트랙 생성 코드
        println!("// 드럼 트랙");
        println!("let mut edm{}_drum_notes = Vec::new();", pattern_idx + 2);
        // 실제로는 노트 데이터를 분석해서 출력해야 함
        println!("// TODO: 드럼 노트 데이터 추가");
        println!("edm{}_tracks.push(RhythmTrack {{", pattern_idx + 2);
        println!("    track_type: TrackType::Drum,");
        println!("    instrument_program: 25, // TR-808 Analog Kit");
        println!("    notes: edm{}_drum_notes,", pattern_idx + 2);
        println!("}});");

        // 베이스 트랙 생성 코드
        println!("// 베이스 트랙");
        println!("let mut edm{}_bass_notes = Vec::new();", pattern_idx + 2);
        println!("// TODO: 베이스 노트 데이터 추가");
        println!("edm{}_tracks.push(RhythmTrack {{", pattern_idx + 2);
        println!("    track_type: TrackType::Bass,");
        println!("    instrument_program: 38, // Synth Bass 1");
        println!("    notes: edm{}_bass_notes,", pattern_idx + 2);
        println!("}});");

        // 반주 트랙 생성 코드
        println!("// 반주 트랙");
        println!("let mut edm{}_synth_notes = Vec::new();", pattern_idx + 2);
        println!("// TODO: 반주 노트 데이터 추가");
        println!("edm{}_tracks.push(RhythmTrack {{", pattern_idx + 2);
        println!("    track_type: TrackType::Accompaniment,");
        println!("    instrument_program: 81, // Sawtooth Lead");
        println!("    notes: edm{}_synth_notes,", pattern_idx + 2);
        println!("}});");

        println!("self.pattern_library.insert(Rhythm::{}, AdvancedRhythmPattern {{", variant_name);
        println!("    length_ticks: 1920,");
        println!("    tracks: edm{}_tracks,", pattern_idx + 2);
        println!("}});");
        println!();
    }
}