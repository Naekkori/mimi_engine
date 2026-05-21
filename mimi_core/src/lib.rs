mod sequencer;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossbeam_channel::{unbounded, Sender, Receiver};
use oxisynth::{SoundFont, Synth};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// 외부(UI 등)에서 오디오 스레드로 보낼 제어 명령
pub enum MimiCommand {
    Play,
    Pause,
    Stop,
    SetKey(i8),       // 조옮김 (예: -1, +2)
    SetTempo(f32),    // 템포 비율 (예: 1.0, 1.2)
    Seek(u32),        // 특정 절대 틱(Tick) 위치로 점프
}

/// 오디오 엔진의 현재 내부 상태
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

/// 재생 추적을 위한 미디 이벤트 구조체 (델타 틱 대신 절대 틱 사용)
pub struct AbsoluteMidiEvent {
    pub absolute_tick: u32,
    pub channel: u8,
    pub message: midly::TrackEventKind<'static>,
}

/// mimi_core 엔진을 제어하기 위한 외부 인터페이스 핸들
pub struct MimiEngineHandle {
    command_tx: Sender<MimiCommand>,
    state: Arc<Mutex<PlayerState>>,
}

impl MimiEngineHandle {
    pub fn send_command(&self, cmd: MimiCommand) -> Result<(), anyhow::Error> {
        self.command_tx.send(cmd).map_err(|e| anyhow::anyhow!("명령 전송 실패: {:?}", e))
    }

    pub fn get_state(&self) -> PlayerState {
        *self.state.lock().unwrap()
    }
}

/// MIMI 엔진 구동 및 오디오 스레드 생성 함수
pub fn spawn_mimi_engine(sf_path: &str) -> Result<MimiEngineHandle, anyhow::Error> {
    let (command_tx, command_rx) = unbounded::<MimiCommand>();
    let player_state = Arc::new(Mutex::new(PlayerState::Stopped));
    let state_clone = Arc::clone(&player_state);

    // 1. CPAL 오디오 출력 장치 및 스트림 설정
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| anyhow::anyhow!("오디오 출력 장치를 찾을 수 없습니다."))?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate();

    // 2. Oxisynth 합성기 초기화 및 사운드폰트 로드
    let mut synth = Synth::default();
    let mut sf_file = std::fs::File::open(sf_path)?;
    let font = SoundFont::load(&mut sf_file).map_err(|e| anyhow::anyhow!("사운드폰트 로드 실패: {:?}", e))?;
    synth.add_font(font, true);

    // 3. 백엔드 전용 내부 타이머 및 재생 상태 변수들
    let mut current_state = PlayerState::Stopped;
    let mut master_key: i8 = 0;
    let mut tempo_scale: f32 = 1.0;
    let mut current_tick: u32 = 0;

    // 음 걸림(Note-stuck) 방지를 위해 현재 켜진 노트를 추적하는 배열
    let mut active_notes: Vec<(u8, u8)> = Vec::new(); // (channel, note)

    // 4. 독립적인 전용 오디오 처리 스레드 생성
    let _audio_thread = thread::spawn(move || {
        // 실제 운영 환경에서는 오디오 스트림 내부 콜백이나 
        // 하이 레졸루션 타이머 루프(1ms)로 동작하게 됩니다.
        loop {
            // 외부 명령 체크 (Non-blocking)
            while let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    MimiCommand::Play => {
                        current_state = PlayerState::Playing;
                        *state_clone.lock().unwrap() = PlayerState::Playing;
                    }
                    MimiCommand::Pause => {
                        current_state = PlayerState::Paused;
                        *state_clone.lock().unwrap() = PlayerState::Paused;
                    }
                    MimiCommand::Stop => {
                        current_state = PlayerState::Stopped;
                        *state_clone.lock().unwrap() = PlayerState::Stopped;
                        current_tick = 0;
                        // 음걸림 방지: 모든 활성화된 노트 강제 Off
                        for (ch, note) in active_notes.drain(..) {
                            // synth.note_off(ch, note);
                        }
                    }
                    MimiCommand::SetKey(key) => {
                        master_key = key;
                        // 실시간 키 변경 시에도 드럼 채널을 제외한 노트 오프 트래킹이 개입되어야 함
                    }
                    MimiCommand::SetTempo(tempo) => {
                        tempo_scale = tempo;
                    }
                    MimiCommand::Seek(tick) => {
                        current_tick = tick;
                        // 구동 중인 모든 노트 끄기 처리 후 점프
                    }
                }
            }

            if current_state == PlayerState::Playing {
                // [정밀 타이밍 렌더링 영역]
                // 1. 정해진 템포와 틱 계산에 따라 미디 이벤트 맵에서 현재 `current_tick`에 해당하는 메시지 추출
                // 2. 추출된 미디 메시지 분석
                //    - Note On 발생 시: 드럼 채널(9번, 10번 소스 등)이 아니면 `note + master_key` 연산 후 synth 전달
                //    - 가사 및 제어 데이터(+d;+ 등)는 파싱하여 렌더러로 보낼 Bridge 채널로 Push
                // 3. 오디오 버퍼 생성 및 장치로 전달

                current_tick += 1; // 가상 진행
            }

            // 고정밀 1ms 주기를 시뮬레이션하기 위한 대기 (실제 하드웨어 타겟 시 cpal 스트림 콜백 내부에서 제어 권장)
            thread::sleep(Duration::from_millis(1));
        }
    });

    Ok(MimiEngineHandle {
        command_tx,
        state: player_state,
    })
}