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
    let (engine_handle, _stream) = spawn_mimi_engine(sf_path, midi_bytes, |p, status| {
        println!("[로딩 진행율] {:.0}% - {}", p * 100.0, status);
    })?;
    println!(
        "[엔진] 초기화 성공. 현재 상태: {:?}",
        engine_handle.get_state()
    );

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
