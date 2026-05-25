use mimi_core::{spawn_mimi_engine, MimiCommand};
use std::thread;
use std::time::Duration;

#[test]
fn engine_test() -> Result<(), anyhow::Error> {
    println!("==================================================");
    println!("      MIMI 노래방 최적화 미디 엔진 통합 테스트     ");
    println!("==================================================");

    // 1. 테스트용 에셋 경로 설정 및 미디 바이너리 로드
    let sf_path = "../assets/soundfont.sf2";
    let midi_path = "../assets/test.mid";

    println!("[로딩] 미디 파일을 읽는 중: {}", midi_path);
    let midi_bytes = std::fs::read(midi_path).map_err(|e| {
        anyhow::anyhow!(
            "테스트 미디 파일을 찾을 수 없습니다. (assets/test.mid 확인 필요): {:?}",
            e
        )
    })?;

    println!("[로딩] 사운드폰트 파일 확인 중: {}", sf_path);
    if !std::path::Path::new(sf_path).exists() {
        return Err(anyhow::anyhow!(
            "테스트 사운드폰트 파일이 없습니다. (assets/soundfont.sf2 확인 필요)"
        ));
    }

    // 2. MIMI 오디오 엔진 가동 (CPAL 하드웨어 스트림 및 오디오 스레드 자동 시작)
    println!("[엔진] MIMI 오디오 엔진 및 CPAL 스트림 초기화 중...");
    let (engine_handle, _stream) = spawn_mimi_engine(sf_path, |p, status| {
        println!("[로딩 진행율] {:.0}% - {}", p * 100.0, status);
    })?;
    println!(
        "[엔진] 초기화 성공. 현재 상태: {:?}",
        engine_handle.get_state()
    );

    // 2.1 미디 바이너리 로드
    println!("[엔진] 미디 파일 주입 중: {}", midi_path);
    engine_handle.send_command(MimiCommand::LoadSong(midi_bytes))?;

    // 3. UI(가사/이벤트) 모니터링 전용 백그라운드 스레드 분리
    // Bevy 엔진이 메인 스레드에서 주기적으로 수신하는 상황을 시뮬레이션합니다.
    let ui_rx = engine_handle.ui_rx.clone();
    thread::spawn(move || {
        while let Ok(ui_event) = ui_rx.recv() {
            match ui_event {
                mimi_core::MidiEngineEvent::SmfKaraokeText { text } => {
                    println!("🎵 [UI 수신 - 가사]: {}", text);
                }
                _ => {}
            }
        }
    });

    // 4. 노래방 실시간 기능 시나리오 테스트
    println!("\n---> [시나리오 1] 재생 시작 (Play Command)");
    engine_handle.send_command(MimiCommand::Play)?;

    // 3초간 정속 연주 청취
    thread::sleep(Duration::from_secs(3));

    println!("\n---> [시나리오 2-1] 동적 조옮김 테스트: +5키 업 ");
    // 음 걸림(Note Stuck) 없이 악기 음정이 자연스럽게 올라가는지 청취
    engine_handle.send_command(MimiCommand::SetKey(5))?;

    thread::sleep(Duration::from_secs(4));
    println!("\n---> [시나리오 2-2] 동적 조옮김 테스트 -5키 다운 ");
    // 음 걸림(Note Stuck) 없이 악기 음정이 자연스럽게 내려가는지 청취
    engine_handle.send_command(MimiCommand::SetKey(-5))?;

    thread::sleep(Duration::from_secs(4));
    engine_handle.send_command(MimiCommand::SetKey(0))?;

    println!("\n---> [시나리오 3] 동적 템포 변경 테스트: 1.3배속 (Fast Tempo)");
    // 연주 속도가 빨라지면서 오디오 싱크가 깨지지 않는지 확인
    engine_handle.send_command(MimiCommand::SetTempo(1.3))?;

    thread::sleep(Duration::from_secs(3));

    println!("\n---> [시나리오 4] 일시정지 테스트 (Pause Command)");
    // 소리가 즉시 뚝 끊기고 잔향이나 걸린 음(Stuck Note)이 없는지 확인
    engine_handle.send_command(MimiCommand::Pause)?;
    println!("[엔진 상태]: {:?}", engine_handle.get_state());

    thread::sleep(Duration::from_secs(2));

    println!("\n---> [시나리오 5] 다시 재생 및 템포 원위치 (Resume & Normal Tempo)");
    engine_handle.send_command(MimiCommand::SetTempo(1.0))?;
    engine_handle.send_command(MimiCommand::Play)?;
    println!("[엔진 상태]: {:?}", engine_handle.get_state());

    thread::sleep(Duration::from_secs(3));

    println!("\n---> [시나리오 6] 연주 완전 정지 및 리셋 (Stop Command)");
    engine_handle.send_command(MimiCommand::Stop)?;
    println!("[엔진 상태]: {:?}", engine_handle.get_state());

    println!("\n==================================================");
    println!("      MIMI 미디 엔진 기본 기능 테스트 완료!      ");
    println!("==================================================");
    
    Ok(())
}

#[test]
fn parse_ky_techno_midi() -> Result<(), anyhow::Error> {
    let midi_path = "../assets/ky_techno.mid";
    let bytes = std::fs::read(midi_path).map_err(|e| {
        anyhow::anyhow!("ky_techno.mid 파일을 찾을 수 없음: {:?}", e)
    })?;
    let smf = midly::Smf::parse(&bytes)?;
    println!("--- ky_techno.mid PPQ: {:?} ---", smf.header.timing);

    // 각 트랙명 선제 정보 파악
    let mut track_names = Vec::new();
    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut track_name = String::new();
        for event in track.iter() {
            if let midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(bytes)) = &event.kind {
                track_name = String::from_utf8_lossy(bytes).to_string();
                break;
            }
        }
        track_names.push(track_name);
    }

    // 마디/박자 및 코드 진행 흐름 파악을 위해 누적 이벤트 리스트 관리
    #[derive(Debug)]
    enum AnalyzeEvent {
        Marker(String),
        Text(String),
        ProgramChange { channel: u8, program: u8 },
        Controller { channel: u8, controller: u8, value: u8 },
        NoteOn { channel: u8, key: u8, vel: u8 },
        NoteOff { channel: u8, key: u8 },
    }

    #[derive(Debug)]
    struct TrackEventWrapper {
        tick: u64,
        track_idx: usize,
        event: AnalyzeEvent,
    }

    let mut events = Vec::new();

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut accum_tick = 0u64;
        for event in track.iter() {
            accum_tick += event.delta.as_int() as u64;
            match &event.kind {
                midly::TrackEventKind::Meta(midly::MetaMessage::Marker(bytes)) => {
                    let text = String::from_utf8_lossy(bytes).to_string();
                    events.push(TrackEventWrapper {
                        tick: accum_tick,
                        track_idx,
                        event: AnalyzeEvent::Marker(text),
                    });
                }
                midly::TrackEventKind::Meta(midly::MetaMessage::Text(bytes)) => {
                    let text = String::from_utf8_lossy(bytes).to_string();
                    events.push(TrackEventWrapper {
                        tick: accum_tick,
                        track_idx,
                        event: AnalyzeEvent::Text(text),
                    });
                }
                midly::TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        midly::MidiMessage::ProgramChange { program } => {
                            events.push(TrackEventWrapper {
                                tick: accum_tick,
                                track_idx,
                                event: AnalyzeEvent::ProgramChange { channel: ch, program: program.as_int() },
                            });
                        }
                        midly::MidiMessage::Controller { controller, value } => {
                            events.push(TrackEventWrapper {
                                tick: accum_tick,
                                track_idx,
                                event: AnalyzeEvent::Controller {
                                    channel: ch,
                                    controller: controller.as_int(),
                                    value: value.as_int(),
                                },
                            });
                        }
                        midly::MidiMessage::NoteOn { key, vel } => {
                            let v = vel.as_int();
                            if v > 0 {
                                events.push(TrackEventWrapper {
                                    tick: accum_tick,
                                    track_idx,
                                    event: AnalyzeEvent::NoteOn { channel: ch, key: key.as_int(), vel: v },
                                });
                            } else {
                                events.push(TrackEventWrapper {
                                    tick: accum_tick,
                                    track_idx,
                                    event: AnalyzeEvent::NoteOff { channel: ch, key: key.as_int() },
                                });
                            }
                        }
                        midly::MidiMessage::NoteOff { key, .. } => {
                            events.push(TrackEventWrapper {
                                tick: accum_tick,
                                track_idx,
                                event: AnalyzeEvent::NoteOff { channel: ch, key: key.as_int() },
                            });
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    // 틱 순서대로 통합 정렬
    events.sort_by_key(|e| e.tick);

    for ev in events {
        let name = &track_names[ev.track_idx];
        match ev.event {
            AnalyzeEvent::Marker(text) => {
                println!("[Marker] Tick {}: {}", ev.tick, text);
            }
            AnalyzeEvent::Text(text) => {
                println!("[Text] Tick {}: {}", ev.tick, text);
            }
            AnalyzeEvent::ProgramChange { channel, program } => {
                println!("[ProgChg] Tick {}: Ch {}, Prog {} (Track {}/{})", ev.tick, channel, program, ev.track_idx, name);
            }
            AnalyzeEvent::Controller { channel, controller, value } => {
                // 주요 컨트롤러(볼륨 7, 익스프레션 11, 팬 10) 위주로만 출력
                if controller == 7 || controller == 10 || controller == 11 {
                    println!("[CtrlChg] Tick {}: Ch {}, CC {}, Val {} (Track {}/{})", ev.tick, channel, controller, value, ev.track_idx, name);
                }
            }
            AnalyzeEvent::NoteOn { channel, key, vel } => {
                println!("[NoteOn] Tick {}: Ch {}, Key {}, Vel {} (Track {}/{})", ev.tick, channel, key, vel, ev.track_idx, name);
            }
            AnalyzeEvent::NoteOff { channel, key } => {
                println!("[NoteOff] Tick {}: Ch {}, Key {} (Track {}/{})", ev.tick, channel, key, ev.track_idx, name);
            }
        }
    }

    Ok(())
}
