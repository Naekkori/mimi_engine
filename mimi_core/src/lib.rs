// mimi_core/src/lib.rs

mod sequencer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{unbounded, Receiver, Sender};
use fluidlite::{IsSettings, IsSamples, Settings, Synth};
pub use sequencer::{MidiEngineEvent, MimiSequencer};
use std::sync::{Arc, Mutex};

/// 외부(UI 등)에서 오디오 엔진으로 보낼 제어 명령
pub enum MimiCommand {
    Play,
    Pause,
    Stop,
    SetKey(i8),    // 조옮김 오프셋 (-6 ~ +6 등)
    SetTempo(f32), // 템포 비율 (1.0 = 정속, 1.2 = 배속)
    Seek(u32),     // 특정 절대 틱(Tick) 위치로 점프
}

/// 오디오 엔진의 현재 내부 상태
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

/// 엔진의 종합 상태 정보 (UI 조회용)
#[derive(Debug, Clone)]
pub struct MimiEngineStatus {
    pub state: PlayerState,
    pub current_tick: u64,
    pub total_tick: u64,
    pub current_time: std::time::Duration,
}

/// 외부 제어용 인터페이스 핸들
pub struct MimiEngineHandle {
    command_tx: Sender<MimiCommand>,
    status: Arc<Mutex<MimiEngineStatus>>,
    pub ui_rx: Receiver<MidiEngineEvent>, // 가사, 리듬 변환 플래그 등을 UI(Bevy) 쪽에서 받아갈 수 있는 채널
    pub ui_tx: Sender<MidiEngineEvent>,
}

impl MimiEngineHandle {
    /// 현재 엔진의 통합 상태를 가져옴
    pub fn get_status(&self) -> Result<MimiEngineStatus, anyhow::Error> {
        self.status
            .lock()
            .map(|s| s.clone())
            .map_err(|e| anyhow::anyhow!("상태 데이터 잠금 획득 실패: {:?}", e))
    }
}

impl MimiEngineHandle {
    pub fn send_command(&self, cmd: MimiCommand) -> Result<(), anyhow::Error> {
        self.command_tx
            .send(cmd)
            .map_err(|e| anyhow::anyhow!("명령 전송 실패: {:?}", e))
    }

    pub fn get_state(&self) -> PlayerState {
        self.status.lock().unwrap().state
    }
}

/// 오디오 콜백 스레드와 통신하며 시퀀싱 및 합성을 전담할 오디오 컨텍스트
struct AudioPlaybackContext {
    sequencer: MimiSequencer,
    synth: Synth,
    command_rx: Receiver<MimiCommand>,
    ui_tx: Sender<MidiEngineEvent>,
    status: Arc<Mutex<MimiEngineStatus>>,

    // 내부 상태 트래킹 변수들
    current_state: PlayerState,
    master_key: i8,
    tempo_scale: f32,
    sample_rate: f64,
    // 음 걸림(Note stuck) 방지용 노트 온 추적 배열 (channel, note)
    active_notes: Vec<(u8, u8)>,
    elapsed_time_sec: f64,
}

impl AudioPlaybackContext {
    /// 실시간으로 외부 명령(`MimiCommand`)을 체크하고 반영합니다.
    fn process_commands(&mut self) {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                MimiCommand::Play => {
                    self.current_state = PlayerState::Playing;
                    self.status.lock().unwrap().state = PlayerState::Playing;
                }
                MimiCommand::Pause => {
                    self.current_state = PlayerState::Paused;
                    self.status.lock().unwrap().state = PlayerState::Paused;
                    self.all_notes_off();
                }
                MimiCommand::Stop => {
                    self.current_state = PlayerState::Stopped;
                    {
                        // 가드가 살아있는 동안 self를 다른 용도로 쓰지 않도록 스코프 분리
                        let mut status = self.status.lock().unwrap();
                        status.state = PlayerState::Stopped;
                        status.current_time = std::time::Duration::from_secs(0);
                    }
                    self.elapsed_time_sec = 0.0;
                    self.all_notes_off();
                    self.sequencer.reset();
                }
                MimiCommand::SetKey(key) => {
                    // 키 변경 시 음걸림 예방을 위해 기존 소리 끄기
                    self.all_notes_off();
                    self.master_key = key;
                }
                MimiCommand::SetTempo(tempo) => {
                    self.tempo_scale = tempo;
                }
                MimiCommand::Seek(tick) => {
                    self.all_notes_off();
                    self.sequencer.current_tick = tick as f64;
                    // 필요한 경우 이벤트 인덱스 역정렬/재탐색 로직 추가 가능
                }
            }
        }
    }

    /// 현재 켜져 있는 모든 노트에 NoteOff를 주입하고 추적 배열을 비웁니다.
    fn all_notes_off(&mut self) {
        for (ch, note) in self.active_notes.drain(..) {
            let _ = self.synth.note_off(ch as u32, note as u32);
        }
    }

    /// 오디오 하드웨어가 요청한 샘플 개수만큼 미디 이벤트를 처리하고 오디오를 합성합니다.
    fn fill_buffer(&mut self, output_buffer: &mut [f32]) {
        // 버퍼 초기화 (이전 루프의 잔상 제거)
        output_buffer.fill(0.0);

        // 1. 명령 처리
        self.process_commands();

        // 스테레오(Left, Right) 채널 처리를 위해 2개 샘플씩 묶어서 루프 돌림
        let num_frames = output_buffer.len() / 2;

        // 1프레임(L/R 한 쌍)당 경과하는 시간(초) 계산
        let sec_per_frame = 1.0 / self.sample_rate;

        // 대여 충돌 방지를 위해 필요한 값들을 미리 복사
        let tempo_scale = self.tempo_scale;

        if self.current_state != PlayerState::Playing {
            // 정지 또는 일시정지 상태면 무음 처리
            for sample in output_buffer.iter_mut() {
                *sample = 0.0;
            }
            return;
        }

        // 프레임 단위로 돌면서 정밀 시퀀싱 진행
        let mut sample_idx = 0;
        for _ in 0..num_frames {

            // 2. 1프레임 분량만큼 시퀀서 전진 및 발생한 이벤트 획득
            let ready_events = self.sequencer.marching(sec_per_frame, tempo_scale);

            // 3. 발생한 미디 이벤트들을 합성기(fluidlite)에 전달
            for event in ready_events {
                match event.inner {
                    MidiEngineEvent::MidiReset => {
                        // fluidlite: system_reset()으로 모든 노트 오프 및 컨트롤러 리셋
                        let _ = self.synth.system_reset();

                        // 표준 기본값 재설정 (볼륨 100, 표현력 127, 팬 64)
                        for ch in 0u32..16 {
                            let _ = self.synth.cc(ch, 7, 100);   // 볼륨
                            let _ = self.synth.cc(ch, 11, 127);  // 표현력(Expression)
                            let _ = self.synth.cc(ch, 10, 64);   // 팬(Pan) 중앙
                            // 뱅크 및 프로그램 초기화
                            let _ = self.synth.cc(ch, 0, 0);     // Bank Select MSB
                            let _ = self.synth.cc(ch, 32, 0);    // Bank Select LSB
                            let _ = self.synth.program_change(ch, 0); // 기본 피아노
                            // 피치 벤드 초기화 (중앙값 8192)
                            let _ = self.synth.pitch_bend(ch, 8192);
                        }
                    }
                    MidiEngineEvent::MidiPlay {
                        channel,
                        kind,
                        is_drum_channel,
                    } => {
                        let target_channel = channel as u32; // 0~15

                        if let midly::TrackEventKind::Midi { message, .. } = kind {
                            match message {
                                midly::MidiMessage::NoteOn { key, vel } => {
                                    let raw_key = key.as_int();
                                    let vel = vel.as_int();

                                    if vel > 0 {
                                        // 조옮김 적용 (드럼 채널은 조옮김 제외)
                                        let final_key = if is_drum_channel {
                                            raw_key
                                        } else {
                                            (raw_key as i8 + self.master_key).clamp(0, 127) as u8
                                        };

                                        // 음걸림 추적 등록 및 노트 온
                                        self.active_notes.push((channel, final_key));
                                        let _ = self.synth.note_on(
                                            target_channel,
                                            final_key as u32,
                                            vel as u32,
                                        );
                                    } else {
                                        // Velocity가 0인 NoteOn은 NoteOff와 동일 처리
                                        let final_key: u8 = if is_drum_channel {
                                            raw_key
                                        } else {
                                            (raw_key as i8 + self.master_key).clamp(0, 127) as u8
                                        };
                                        self.active_notes.retain(|&(ch, n)| {
                                            !(ch == channel && n == final_key)
                                        });
                                        let _ = self.synth.note_off(
                                            target_channel,
                                            final_key as u32,
                                        );
                                    }
                                }
                                midly::MidiMessage::NoteOff { key, .. } => {
                                    let raw_key = key.as_int();
                                    let final_key = if is_drum_channel {
                                        raw_key
                                    } else {
                                        (raw_key as i8 + self.master_key).clamp(0, 127) as u8
                                    };
                                    self.active_notes.retain(|&(ch, n)| {
                                        !(ch == channel && n == final_key)
                                    });
                                    let _ = self.synth.note_off(
                                        target_channel,
                                        final_key as u32,
                                    );
                                }
                                midly::MidiMessage::Controller { controller, value } => {
                                    let _ = self.synth.cc(
                                        target_channel,
                                        controller.as_int() as u32,
                                        value.as_int() as u32,
                                    );
                                }
                                midly::MidiMessage::ProgramChange { program } => {
                                    let _ = self.synth.program_change(
                                        target_channel,
                                        u32::from(program.as_int()),
                                    );
                                }
                                midly::MidiMessage::PitchBend { bend } => {
                                    // fluidlite pitch_bend: 0~16383, 중앙 8192
                                    // midly bend.as_int()는 -8192~8191 범위이므로 +8192 오프셋 적용
                                    let raw = bend.as_int(); // i16, -8192~8191
                                    let value = (raw as i32 + 8192).clamp(0, 16383) as u32;
                                    let _ = self.synth.pitch_bend(target_channel, value);
                                }
                                _ => {}
                            }
                        }
                    }
                    other_event => {
                        let _ = self.ui_tx.send(other_event);
                    }
                }
            }

            // 4. 오디오 합성 (fluidlite: write()는 IsSamples trait으로 인터리브 스테레오 출력)
            let buf: &mut [f32] = &mut output_buffer[sample_idx..sample_idx + 2];
            let _ = buf.write_samples(&self.synth);

            sample_idx += 2;

            // 5. 연주가 끝났는지 확인 (모든 이벤트 처리 완료 시)
            if self.sequencer.is_finished() {
                self.current_state = PlayerState::Stopped;
                {
                    // status 가드가 self 메서드 호출 전에 드롭되도록 스코프 처리
                    let mut status = self.status.lock().unwrap();
                    status.state = PlayerState::Stopped;
                    status.current_tick = 0;
                    status.current_time = std::time::Duration::from_secs(0);
                }
                self.elapsed_time_sec = 0.0;
                self.all_notes_off();
                self.sequencer.reset();

                // 버퍼의 남은 구간을 무음으로 채움
                output_buffer[sample_idx..].fill(0.0);
                break;
            }
        }

        // 연주 중일 때 실제 경과 시간 업데이트
        if self.current_state == PlayerState::Playing {
            self.elapsed_time_sec += num_frames as f64 / self.sample_rate;
        }

        // 한 주기의 오디오 데이터 생성이 끝난 후 공유 상태 업데이트
        {
            let mut status = self.status.lock().unwrap();
            status.current_tick = self.sequencer.current_tick as u64;
            status.total_tick = self.sequencer.total_ticks as u64;
            status.current_time = std::time::Duration::from_secs_f64(self.elapsed_time_sec);
        }

        // 한 주기의 오디오 데이터 생성이 끝난 후 UI에 전송
        let _ = self.ui_tx.send(MidiEngineEvent::TickUpdate {
            current_tick: self.sequencer.current_tick as u64,
            total_tick: self.sequencer.total_ticks as u64,
        });
    }
}

/// MIMI 엔진 구동 및 CPAL 하드웨어 오디오 스트림 활성화 함수
pub fn spawn_mimi_engine(
    sf_path: &str,
    midi_bytes: Vec<u8>,
) -> Result<(MimiEngineHandle, cpal::Stream), anyhow::Error> {
    let (command_tx, command_rx) = unbounded::<MimiCommand>();
    let (ui_tx, ui_rx) = unbounded::<MidiEngineEvent>();

    // 공유 상태 객체 초기화
    let player_status = Arc::new(Mutex::new(MimiEngineStatus {
        state: PlayerState::Stopped,
        current_tick: 0,
        total_tick: 0,
        current_time: std::time::Duration::from_secs(0),
    }));
    let status_clone = Arc::clone(&player_status);

    // 1. CPAL을 통한 오디오 하드웨어 장치 열기
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("오디오 출력 장치를 찾을 수 없습니다."))?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate() as f64;
    let channels = config.channels();

    // 스테레오 채널 설정 확인
    let cpal_config: cpal::StreamConfig = config.into();
    if channels != 2u16 {
        return Err(anyhow::anyhow!(
            "MIMI 엔진은 현재 스테레오(2채널) 출력 장치만 지원합니다."
        ));
    }

    // 2. fluidlite 합성기 설정
    let settings = Settings::new()
        .map_err(|e| anyhow::anyhow!("FluidLite Settings 생성 실패: {:?}", e))?;

    // 샘플레이트를 Settings의 num 설정으로 사전 주입
    if let Some(sr_setting) = settings.num("synth.sample-rate") {
        sr_setting.set(sample_rate as f64);
    }

    let synth = Synth::new(settings)
        .map_err(|e| anyhow::anyhow!("FluidLite Synth 생성 실패: {:?}", e))?;

    // 사운드폰트 로드 (fluidlite는 AsRef<Path> 경로 문자열을 직접 받음)
    synth.sfload(sf_path, true)
        .map_err(|e| anyhow::anyhow!("사운드폰트 로드 실패: {:?}", e))?;

    // 3. 시퀀서 준비
    let sequencer = MimiSequencer::from_byte(&midi_bytes)?;

    // 전체 틱 정보를 상태에 미리 반영
    player_status.lock().unwrap().total_tick = sequencer.total_ticks as u64;

    // 4. 오디오 콜백 스레드로 넘겨줄 컨텍스트 객체 생성
    let mut playback_context = AudioPlaybackContext {
        sequencer,
        synth,
        command_rx,
        ui_tx: ui_tx.clone(),
        status: status_clone,
        current_state: PlayerState::Stopped,
        master_key: 0,
        tempo_scale: 1.0,
        sample_rate,
        active_notes: Vec::new(),
        elapsed_time_sec: 0.0,
    };

    // 5. CPAL 오디오 스트림 생성 (독립 하드웨어 스레드에서 무한 반복 호출됨)
    let error_callback = |err| eprintln!("오디오 스트림 에러 발생: {}", err);
    let stream = device.build_output_stream(
        &cpal_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            playback_context.fill_buffer(data);
        },
        error_callback,
        None,
    )?;

    // 스트림 즉시 가동
    stream.play()?;

    let handle = MimiEngineHandle {
        command_tx,
        status: player_status,
        ui_rx,
        ui_tx,
    };

    // cpal::Stream의 수명이 다하면 소리가 끊기므로 Handle과 함께 반환하여 상위 메인에서 관리하도록 함
    Ok((handle, stream))
}
