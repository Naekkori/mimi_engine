// mimi_core/src/lib.rs

mod sequencer;
mod rhythm_engine;
use crossbeam_channel::{unbounded, Receiver, Sender};
use fluidlite::{IsSettings, IsSamples, Settings, Synth};
use midly::TrackEventKind;
pub use sequencer::{MidiEngineEvent, MidiFormat, MimiSequencer};
pub use rhythm_engine::{Rhythm, RhythmEngine, MidiNote, BsChordEvent};
use std::sync::{Arc, Mutex};

// 템포 배율 제한 (0.2 ~ 5.0)
pub const TEMPO_MIN: f32 = 0.2;
pub const TEMPO_MAX: f32 = 5.0;

// 조옮김 반음 제한 (-15 ~ +15)
pub const KEY_MIN: i8 = -15;
pub const KEY_MAX: i8 = 15;

// 마스터 볼륨 제한 (0 ~ 100)
pub const VOLUME_MIN: u8 = 0;
pub const VOLUME_MAX: u8 = 100;

/// 외부(UI 등)에서 오디오 엔진으로 보낼 제어 명령
pub enum MimiCommand {
    Play,
    Pause,
    Stop,
    SetKey(i8),     // 조옮김 오프셋 (KEY_MIN ~ KEY_MAX)
    SetTempo(f32),  // 템포 비율 (TEMPO_MIN ~ TEMPO_MAX)
    SetVolume(u8),  // 마스터 볼륨 (VOLUME_MIN ~ VOLUME_MAX)
    Seek(u32),      // 특정 절대 틱(Tick) 위치로 점프
    LoadSong(Vec<u8>), // 새로운 MIDI 바이너리를 시퀀서에 주입 후 리셋 대기
    SetRhythm(Rhythm), // 실시간 리듬 모드 변경 (Original, Disco, GoGo, Techno, Dance, Hiphop, Jitterbug, Edm)
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
    // 현재 파라미터 값 (엔진이 권위 있는 값을 보유)
    pub tempo: f32,
    pub key: i8,
    pub volume: u8,
    pub current_rhythm: Rhythm,
    // 해당 곡에서 코드 진행 추출용 $BS(또는 베이스 라인)이 실제로 검출되었는지 여부
    pub is_bs_detected: bool,
    // 미디파일 의 현재 템포
    pub current_tempo: i32,
    // 미디파일에 정의된 원곡 키 시그니처 (샤프/플랫 수, 단조 여부)
    pub song_key_sig: Option<(i8, bool)>,
}

/// 외부 제어용 인터페이스 핸들
pub struct MimiEngineHandle {
    command_tx: Sender<MimiCommand>,
    status: Arc<Mutex<MimiEngineStatus>>,
    pub ui_rx: Receiver<MidiEngineEvent>, // 가사, 리듬 변환 플래그 등을 UI(Bevy) 쪽에서 받아갈 수 있는 채널
    pub ui_tx: Sender<MidiEngineEvent>,
}
// 엔진정보
#[derive(Debug, Clone)]
pub struct MimiEngineInfo {
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
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
pub struct AudioPlaybackContext {
    sequencer: MimiSequencer,
    synth_a: Synth,
    synth_b: Synth,
    command_rx: Receiver<MimiCommand>,
    ui_tx: Sender<MidiEngineEvent>,
    status: Arc<Mutex<MimiEngineStatus>>,

    current_state: PlayerState,
    master_key: i8,
    tempo_scale: f32,
    master_volume: u8, // 마스터 볼륨 (VOLUME_MIN ~ VOLUME_MAX)
    sample_rate: f64,
    active_notes: Vec<(u8, u8, u8)>,
    elapsed_time_sec: f64,

    midi_format: MidiFormat,
    bank_msb: [[u8; 16]; 2],
    bank_lsb: [[u8; 16]; 2],
    drum_channels: [[bool; 16]; 2],
    channel_velocities: [[u8; 16]; 2],
    // RPN/NRPN 상태 추적 (CC#6 = Data Entry MSB 처리용)
    rpn_msb: [[u8; 16]; 2],
    rpn_lsb: [[u8; 16]; 2],
    nrpn_active: [[bool; 16]; 2],

    // 실시간 리듬변환 개입 모듈
    rhythm_engine: RhythmEngine,
    // 현재 생성되어 연주되는 리듬 반주 노트 목록
    generated_rhythm_notes: Vec<MidiNote>,
    // 다음 연주해야 할 리듬 노트 인덱스
    next_rhythm_note_index: usize,
    // 포트 A 리듬 변환 채널 뮤트 마스크 (각 비트가 1이면 뮤트)
    rhythm_mute_mask: u16,
    // 사용자가 UI 등에서 수동으로 선택 및 지정 지정해 놓은 배후 리듬 모드
    user_selected_rhythm: Rhythm,
}

impl AudioPlaybackContext {
    /// 실시간으로 외부 명령(`MimiCommand`)을 체크하고 반영합니다.
    fn process_commands(&mut self) {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                MimiCommand::Play => {
                    self.current_state = PlayerState::Playing;
                    self.status.lock().unwrap().state = PlayerState::Playing;
                    
                    // 재생을 다시 시작하는 시점에 리듬 인덱스도 재생 포인터 상태에 맞춰 초기화 유도
                    let current_tick = self.sequencer.current_tick as u32;
                    self.next_rhythm_note_index = self.generated_rhythm_notes
                        .partition_point(|n| n.tick < current_tick);
                }
                MimiCommand::Pause => {
                    self.current_state = PlayerState::Paused;
                    self.status.lock().unwrap().state = PlayerState::Paused;
                    self.all_notes_off();
                }
                MimiCommand::Stop => {
                    self.current_state = PlayerState::Stopped;
                    
                    // 정지 시 원래 사용자가 선택했던 리듬 복원
                    self.rhythm_engine.current_rhythm = self.user_selected_rhythm;
                    self.rhythm_mute_mask = 0; // 마스크 초기화

                    {
                        // 가드가 살아있는 동안 self를 다른 용도로 쓰지 않도록 스코프 분리
                        let mut status = self.status.lock().unwrap();
                        status.state = PlayerState::Stopped;
                        status.current_time = std::time::Duration::from_secs(0);
                        status.current_tick = 0;
                        status.current_rhythm = self.rhythm_engine.current_rhythm; // 복원상태 반영
                    }
                    self.elapsed_time_sec = 0.0;
                    self.all_notes_off();
                    self.sequencer.reset();
                    self.next_rhythm_note_index = 0;
                }
                MimiCommand::SetKey(key) => {
                    // 키 변경 시 음걸림 예방을 위해 기존 소리 끄기
                    self.all_notes_off();
                    self.master_key = key.clamp(KEY_MIN, KEY_MAX);
                    self.status.lock().unwrap().key = self.master_key;
                }
                MimiCommand::SetTempo(tempo) => {
                    self.tempo_scale = tempo.clamp(TEMPO_MIN, TEMPO_MAX);
                    self.status.lock().unwrap().tempo = self.tempo_scale;
                }
                MimiCommand::SetVolume(vol) => {
                    self.master_volume = vol.clamp(VOLUME_MIN, VOLUME_MAX);
                    // fluidlite gain: 0.0 ~ 10.0 범위, 100 -> 1.0 (기본값)으로 매핑
                    let gain = self.master_volume as f32 / 100.0 * 2.0;
                    self.synth_a.set_gain(gain);
                    self.synth_b.set_gain(gain);
                    self.status.lock().unwrap().volume = self.master_volume;
                }
                MimiCommand::Seek(tick) => {
                    self.all_notes_off();
                    
                    // 신디사이저 리셋
                    let _ = self.synth_a.system_reset();
                    let _ = self.synth_b.system_reset();

                    // 내부상태 초기화
                    self.bank_msb = [[0u8; 16]; 2];
                    self.bank_lsb = [[0u8; 16]; 2];
                    self.drum_channels = [[false; 16]; 2];
                    self.drum_channels[0][9] = true;
                    self.drum_channels[1][9] = true;
                    self.midi_format = self.sequencer.format;

                    // 시퀀서 위치 이동 (인덱스 + 템포복원)
                    self.sequencer.seek_to(tick);
                    
                    // 리듬 변환 발생 노트 포인터 복원
                    self.next_rhythm_note_index = self.generated_rhythm_notes
                        .partition_point(|n| n.tick < tick);

                    #[derive(Clone, Copy)]
                    struct ChannelSetup {
                        program: Option<u8>,
                        bank_msb: u8,
                        bank_lsb: u8,
                        volume: Option<u8>,
                        pan: Option<u8>,
                        expression: Option<u8>,
                        pitch_bend: Option<u16>,
                        pitch_bend_range: Option<u8>,
                    }

                    let mut channel_presets = [[ChannelSetup {
                        program: None,
                        bank_msb: 0,
                        bank_lsb: 0,
                        volume: None,
                        pan: None,
                        expression: None,
                        pitch_bend: None,
                        pitch_bend_range: None,
                    }; 16]; 2];

                    // seek_to 전용 RPN/NRPN 상태 추적
                    let mut seek_rpn_msb = [[127u8; 16]; 2];
                    let mut seek_rpn_lsb = [[127u8; 16]; 2];
                    let mut seek_nrpn_active = [[false; 16]; 2];

                    // Seek 지점 이전 이벤트 중 상태성 이벤트만 추적
                    for i in 0..self.sequencer.current_event_index {
                        let event = &self.sequencer.event[i];
                        match &event.inner {
                            MidiEngineEvent::MidiReset => {
                                // 미디 리셋 시 누적 상태들도 모두 초기화
                                channel_presets = [[ChannelSetup {
                                    program: None,
                                    bank_msb: 0,
                                    bank_lsb: 0,
                                    volume: None,
                                    pan: None,
                                    expression: None,
                                    pitch_bend: None,
                                    pitch_bend_range: None,
                                }; 16]; 2];
                                self.bank_msb = [[0u8; 16]; 2];
                                self.bank_lsb = [[0u8; 16]; 2];
                                self.drum_channels = [[false; 16]; 2];
                                self.drum_channels[0][9] = true;
                                self.drum_channels[1][9] = true;
                                seek_rpn_msb = [[127u8; 16]; 2];
                                seek_rpn_lsb = [[127u8; 16]; 2];
                                seek_nrpn_active = [[false; 16]; 2];
                            }
                            MidiEngineEvent::MidiPlay { port, channel, is_drum_channel: _, kind } => {
                                let p = (*port).min(1) as usize;
                                let ch = (*channel).min(15) as usize;
                                if let TrackEventKind::Midi { message, .. } = kind {
                                    match message {
                                        midly::MidiMessage::Controller { controller, value } => {
                                            let cc = controller.as_int();
                                            let val = value.as_int();
                                            match cc {
                                                0 => {
                                                    channel_presets[p][ch].bank_msb = val;
                                                    self.bank_msb[p][ch] = val;
                                                    if ch != 9 {
                                                        // 이미 드럼 채널로 격상된 경우 덮어쓰기 방지 기능 적용
                                                        if !self.drum_channels[p][ch] {
                                                            if val == 120 || val == 126 || val == 127 {
                                                                self.drum_channels[p][ch] = true;
                                                            } else {
                                                                self.drum_channels[p][ch] = false;
                                                            }
                                                        }
                                                    }
                                                }
                                                32 => {
                                                    channel_presets[p][ch].bank_lsb = val;
                                                    self.bank_lsb[p][ch] = val;
                                                }
                                                7 => {
                                                    channel_presets[p][ch].volume = Some(val);
                                                }
                                                10 => {
                                                    channel_presets[p][ch].pan = Some(val);
                                                }
                                                11 => {
                                                    channel_presets[p][ch].expression = Some(val);
                                                }
                                                // RPN/NRPN 상태 추적 후 Pitch Bend Range 판별
                                                6 => {
                                                    if !seek_nrpn_active[p][ch]
                                                        && seek_rpn_msb[p][ch] == 0
                                                        && seek_rpn_lsb[p][ch] == 0
                                                    {
                                                        channel_presets[p][ch].pitch_bend_range = Some(val);
                                                    }
                                                }
                                                99 => {
                                                    seek_nrpn_active[p][ch] = true;
                                                }
                                                101 => {
                                                    seek_rpn_msb[p][ch] = val;
                                                    seek_nrpn_active[p][ch] = false;
                                                }
                                                100 => {
                                                    seek_rpn_lsb[p][ch] = val;
                                                    seek_nrpn_active[p][ch] = false;
                                                }
                                                _ => {}
                                            }
                                        }
                                        midly::MidiMessage::ProgramChange { program } => {
                                            channel_presets[p][ch].program = Some(program.as_int());
                                        }
                                        midly::MidiMessage::PitchBend { bend } => {
                                            let value = (bend.as_int() as i32 + 8192).clamp(0, 16383) as u16;
                                            channel_presets[p][ch].pitch_bend = Some(value);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            MidiEngineEvent::SetDrumChannel { port, channel, is_drum } => {
                                let p = (*port).min(1) as usize;
                                let ch = (*channel).min(15) as usize;
                                self.drum_channels[p][ch] = *is_drum;
                            }
                            _ => {}
                        }
                    }

                    // 추적 완료 후 최종 Preset 상태를 2개 신디사이저 포트에 각각 적용
                    for p in 0..2 {
                        let synth = if p == 0 { &mut self.synth_a } else { &mut self.synth_b };
                        for ch in 0..16 {
                            let setup = &channel_presets[p][ch];
                            let is_drum = self.drum_channels[p][ch];

                            // 1. 드럼 또는 포맷별 뱅크 설정
                            let resolved_bank: u32 = if is_drum {
                                128 << 7
                            } else {
                                match self.midi_format {
                                    MidiFormat::GM => 0,
                                    MidiFormat::GS => 0,
                                    MidiFormat::XG => (setup.bank_msb as u32) << 7,
                                }
                            };
                            let _ = synth.bank_select(ch as u32, resolved_bank >> 7);
                            let _ = synth.cc(ch as u32, 32, setup.bank_lsb as u32);

                            // 2. 프로그램 체인지(악기 변경) 적용
                            let prog = setup.program.unwrap_or(0);
                            if is_drum {
                                let _ = synth.bank_select(ch as u32, 128);
                            } else {
                                let _ = synth.bank_select(ch as u32, resolved_bank >> 7);
                                let _ = synth.cc(ch as u32, 32, setup.bank_lsb as u32);
                            }
                            let _ = synth.program_change(ch as u32, prog as u32);

                            // 3. 주요 제어 컨트롤러(볼륨, 팬, 익스프레션) 복원
                            if let Some(vol) = setup.volume {
                                let _ = synth.cc(ch as u32, 7, vol as u32);
                            }
                            if let Some(pan) = setup.pan {
                                let _ = synth.cc(ch as u32, 10, pan as u32);
                            }
                            if let Some(exp) = setup.expression {
                                let _ = synth.cc(ch as u32, 11, exp as u32);
                            }

                            // 4. 피치 벤드 범위(RPN) 복원
                            if let Some(pbr) = setup.pitch_bend_range {
                                let _ = synth.pitch_wheel_sens(ch as u32, pbr as u32);
                            }

                            // 5. 피치 벤드 복원
                            if let Some(pb) = setup.pitch_bend {
                                let _ = synth.pitch_bend(ch as u32, pb as u32);
                            }
                        }
                    }
                    
                    // 일시정지 상태에서도 Seek 업데이트 UI 에 갱신
                    {
                        let mut status = self.status.lock().unwrap();
                        status.current_tick = tick as u64;
                        let elapsed_sec = (tick as f64) * self.sequencer.microseconds_per_tick / 1_000_000.0;
                        status.current_time = std::time::Duration::from_secs_f64(elapsed_sec);
                        // seek_to 내부에서 복원된 템포를 status에도 반영
                        let restored_tempo = (self.sequencer.microseconds_per_tick * self.sequencer.ppq as f64) as i32;
                        status.current_tempo = restored_tempo;
                    }
                    let elapsed_sec = (tick as f64) * self.sequencer.microseconds_per_tick / 1_000_000.0;
                    self.elapsed_time_sec = elapsed_sec;
                    // 볼륨 게인 재적용
                    let gain = self.master_volume as f32 / 100.0 * 2.0;
                    self.synth_a.set_gain(gain);
                    self.synth_b.set_gain(gain);
                }
                MimiCommand::LoadSong(bytes) => {
                    self.current_state = PlayerState::Stopped;
                    self.elapsed_time_sec = 0.0;
                    self.all_notes_off();

                    // 곡 로드 시 채널 마스킹 필터 및 원곡 리듬 상태 강제 초기화
                    self.rhythm_mute_mask = 0;

                    // 1. 신디사이저 하드웨어 완벽 초기화 (이전 곡 잔재 제거)
                    let _ = self.synth_a.system_reset();
                    let _ = self.synth_b.system_reset();

                    // 모든 MIDI 채널에 기본 컨트롤 초기값 강제 적용 (Reset All Controllers & Default Volume)
                    for synth in [&self.synth_a, &self.synth_b] {
                        for ch in 0u32..16 {
                            let _ = synth.cc(ch, 121, 0); // Reset All Controllers
                            let _ = synth.cc(ch, 0, 0);   // Bank Select MSB
                            let _ = synth.cc(ch, 32, 0);  // Bank Select LSB
                            let _ = synth.program_change(ch, 0); // Default Grand Piano
                            let _ = synth.cc(ch, 7, 100);  // Channel Volume Default
                            let _ = synth.cc(ch, 11, 127); // Expression Default
                            let _ = synth.cc(ch, 10, 64);  // Pan Default (Center)
                            let _ = synth.pitch_bend(ch, 8192); // Pitch Bend Center
                        }
                    }

                    // 내부상태 초기화
                    self.bank_msb = [[0u8; 16]; 2];
                    self.bank_lsb = [[0u8; 16]; 2];
                    self.rpn_msb = [[127u8; 16]; 2];
                    self.rpn_lsb = [[127u8; 16]; 2];
                    self.nrpn_active = [[false; 16]; 2];
                    self.drum_channels = {
                        let mut dc = [[false; 16]; 2];
                        dc[0][9] = true;
                        dc[1][9] = true;
                        dc
                    };
                    self.channel_velocities = [[0u8; 16]; 2];

                    if let Ok(new_seq) = MimiSequencer::from_byte(&bytes) {
                        self.midi_format = new_seq.format;
                        // $BS 메타 트랙 존재 유무 식별
                        let bs_detected = new_seq.is_bs_track_detected;
                        self.sequencer = new_seq;
                        
                        let mut status = self.status.lock().unwrap();
                        status.state = PlayerState::Stopped;
                        status.current_tick = 0;
                        status.total_tick = self.sequencer.total_ticks as u64;
                        status.current_time = std::time::Duration::from_secs(0);
                        status.is_bs_detected = bs_detected;
                        // 새 곡 로드 시 이전 곡의 키 시그니처 초기화
                        status.song_key_sig = None;
                    }
                    
                    // 신규 곡의 음량 게인 강제 재조정
                    let gain = self.master_volume as f32 / 100.0 * 2.0;
                    self.synth_a.set_gain(gain);
                    self.synth_b.set_gain(gain);

                    // 새로운 곡 주입 시 리듬 반주 틱 생성용 타임라인 구성
                    if self.rhythm_engine.current_rhythm != Rhythm::Original {
                        self.generated_rhythm_notes = self.rhythm_engine.generate_accompaniment_tracks(
                            self.sequencer.total_ticks,
                            &self.sequencer.chord_timeline,
                            self.sequencer.ppq,
                        );
                    } else {
                        self.generated_rhythm_notes.clear();
                    }
                    self.next_rhythm_note_index = 0;
                }
                MimiCommand::SetRhythm(rhythm) => {
                    // 사용자가 수동 지정한 리듬 보관 유지
                    self.user_selected_rhythm = rhythm;

                    // 키 및 리듬 상태 갱신
                    self.rhythm_engine.current_rhythm = rhythm;
                    self.status.lock().unwrap().current_rhythm = rhythm;

                    // 실시간으로 모든 이전 음 찌꺼기 끄기 (Note Stuck 방지)
                    // port 0, 1 전체 신디사이저 포트에 하드웨어 레벨 All Notes Off 및 All Sound Off
                    for port in 0..2 {
                        let synth = if port == 0 { &self.synth_a } else { &self.synth_b };
                        for ch in 0..16 {
                            let _ = synth.cc(ch as u32, 123, 0); // All Notes Off
                            let _ = synth.cc(ch as u32, 120, 0); // All Sound Off
                        }
                    }
                    self.active_notes.clear();

                    if rhythm != Rhythm::Original {
                        // 선택된 리듬에 맞춰 실시간으로 대리 리듬 악보 생성해 냄
                        self.generated_rhythm_notes = self.rhythm_engine.generate_accompaniment_tracks(
                            self.sequencer.total_ticks,
                            &self.sequencer.chord_timeline,
                            self.sequencer.ppq,
                        );
                        // 현재 틱 이후의 첫 오프셋 지점 탐색
                        let current_tick = self.sequencer.current_tick as u32;
                        self.next_rhythm_note_index = self.generated_rhythm_notes
                            .partition_point(|n| n.tick < current_tick);
                    } else {
                        self.generated_rhythm_notes.clear();
                        self.next_rhythm_note_index = 0;
                        self.restore_original_states();
                    }
                }
            }
        }
    }

    /// 현재 켜져 있는 모든 노트에 NoteOff를 주입하고 추적 배열을 비웁니다.
    fn all_notes_off(&mut self) {
        for (port,ch,note) in self.active_notes.drain(..) {
            if port == 0{
                let _ = self.synth_a.note_off(ch as u32, note as u32);
            }else{
                let _ = self.synth_b.note_off(ch as u32, note as u32);
            }
        }
    }

    /// [오리지널 복원] 원곡 모드로 변경될 때, 현재 틱(Tick) 이전까지 생성되었던
    /// 원곡의 프로그램 정보, 컨트롤러, 뱅크 상태를 시퀀서 타임라인 역추적해서 완벽 복구
    fn restore_original_states(&mut self) {
        #[derive(Clone, Copy)]
        struct ChannelSetup {
            program: Option<u8>,
            bank_msb: u8,
            bank_lsb: u8,
            volume: Option<u8>,
            pan: Option<u8>,
            expression: Option<u8>,
            pitch_bend: Option<u16>,
            pitch_bend_range: Option<u8>,
        }

        let mut channel_presets = [[ChannelSetup {
            program: None,
            bank_msb: 0,
            bank_lsb: 0,
            volume: None,
            pan: None,
            expression: None,
            pitch_bend: None,
            pitch_bend_range: None,
        }; 16]; 2];

        // seek_to 전용 RPN/NRPN 상태 추적 (CC#6 = Pitch Bend Range 판별용)
        let mut seek_rpn_msb = [[127u8; 16]; 2];
        let mut seek_rpn_lsb = [[127u8; 16]; 2];
        let mut seek_nrpn_active = [[false; 16]; 2];

        // 현재 재생 틱 이전의 미디 설정 이벤트 스캔
        let current_index = self.sequencer.current_event_index;
        for i in 0..current_index {
            let event = &self.sequencer.event[i];
            match &event.inner {
                MidiEngineEvent::MidiReset => {
                    channel_presets = [[ChannelSetup {
                        program: None,
                        bank_msb: 0,
                        bank_lsb: 0,
                        volume: None,
                        pan: None,
                        expression: None,
                        pitch_bend: None,
                        pitch_bend_range: None,
                    }; 16]; 2];
                    seek_rpn_msb = [[127u8; 16]; 2];
                    seek_rpn_lsb = [[127u8; 16]; 2];
                    seek_nrpn_active = [[false; 16]; 2];
                }
                MidiEngineEvent::MidiPlay { port, channel, is_drum_channel: _, kind } => {
                    let p = (*port).min(1) as usize;
                    let ch = (*channel).min(15) as usize;
                    if let TrackEventKind::Midi { message, .. } = kind {
                        match message {
                            midly::MidiMessage::Controller { controller, value } => {
                                let cc = controller.as_int();
                                let val = value.as_int();
                                match cc {
                                    0 => channel_presets[p][ch].bank_msb = val,
                                    32 => channel_presets[p][ch].bank_lsb = val,
                                    7 => channel_presets[p][ch].volume = Some(val),
                                    10 => channel_presets[p][ch].pan = Some(val),
                                    11 => channel_presets[p][ch].expression = Some(val),
                                    // RPN/NRPN 상태 추적 후 Pitch Bend Range 판별
                                    6 => {
                                        if !seek_nrpn_active[p][ch]
                                            && seek_rpn_msb[p][ch] == 0
                                            && seek_rpn_lsb[p][ch] == 0
                                        {
                                            channel_presets[p][ch].pitch_bend_range = Some(val);
                                        }
                                    }
                                    99 => {
                                        seek_nrpn_active[p][ch] = true;
                                    }
                                    101 => {
                                        seek_rpn_msb[p][ch] = val;
                                        seek_nrpn_active[p][ch] = false;
                                    }
                                    100 => {
                                        seek_rpn_lsb[p][ch] = val;
                                        seek_nrpn_active[p][ch] = false;
                                    }
                                    _ => {}
                                }
                            }
                            midly::MidiMessage::ProgramChange { program } => {
                                channel_presets[p][ch].program = Some(program.as_int());
                            }
                            midly::MidiMessage::PitchBend { bend } => {
                                let value = (bend.as_int() as i32 + 8192).clamp(0, 16383) as u16;
                                channel_presets[p][ch].pitch_bend = Some(value);
                            }
                            _ => {}
                        }
                    }
                }
                MidiEngineEvent::SetDrumChannel { port, channel, is_drum } => {
                    let p = (*port).min(1) as usize;
                    let ch = (*channel).min(15) as usize;
                    self.drum_channels[p][ch] = *is_drum;
                }
                _ => {}
            }
        }

        // 역추적한 프리셋들을 실제 신디사이저에 동기화
        for p in 0..2 {
            let synth = if p == 0 { &mut self.synth_a } else { &mut self.synth_b };
            for ch in 0..16 {
                let setup = &channel_presets[p][ch];
                let is_drum = self.drum_channels[p][ch];

                // 프로그램 변경 역추적 데이터가 실존하는 경우에만 뱅크/프로그램 지정
                if let Some(prog) = setup.program {
                    let resolved_bank: u32 = if is_drum {
                        128 << 7
                    } else {
                        match self.midi_format {
                            MidiFormat::GM => 0,
                            MidiFormat::GS => 0,
                            MidiFormat::XG => (setup.bank_msb as u32) << 7,
                        }
                    };
                    if is_drum {
                        let _ = synth.bank_select(ch as u32, 128);
                    } else {
                        let _ = synth.bank_select(ch as u32, resolved_bank >> 7);
                        let _ = synth.cc(ch as u32, 32, setup.bank_lsb as u32);
                    }
                    let _ = synth.program_change(ch as u32, prog as u32);
                }

                if let Some(vol) = setup.volume {
                    let _ = synth.cc(ch as u32, 7, vol as u32);
                }
                if let Some(pan) = setup.pan {
                    let _ = synth.cc(ch as u32, 10, pan as u32);
                }
                if let Some(exp) = setup.expression {
                    let _ = synth.cc(ch as u32, 11, exp as u32);
                }
                // RPN 피치 벤드 범위 복원
                if let Some(pbr) = setup.pitch_bend_range {
                    let _ = synth.pitch_wheel_sens(ch as u32, pbr as u32);
                }
                if let Some(pb) = setup.pitch_bend {
                    let _ = synth.pitch_bend(ch as u32, pb as u32);
                }
            }
        }
    }

    /// 오디오 하드웨어가 요청한 샘플 개수만큼 미디 이벤트를 처리하고 오디오를 합성합니다.
    pub fn fill_buffer(&mut self, output_buffer: &mut [f32]) {
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
                        let _ = self.synth_a.system_reset();
                        let _ = self.synth_b.system_reset();

                        self.midi_format = self.sequencer.format;
                        self.bank_msb = [[0u8; 16]; 2];
                        self.bank_lsb = [[0u8; 16]; 2];
                        self.drum_channels = [[false; 16]; 2];
                        self.drum_channels[0][9] = true;
                        self.drum_channels[1][9] = true;
                        self.drum_channels[1][15] = true; // 리듬 드럼 격리 전용 채널 활성화 락
                        self.rpn_msb = [[127u8; 16]; 2];
                        self.rpn_lsb = [[127u8; 16]; 2];
                        self.nrpn_active = [[false; 16]; 2];

                        for ch in 0u32..16{
                            let _ = self.synth_a.cc(ch, 0, 0);
                            let _ = self.synth_a.cc(ch, 32, 0);
                            let _ = self.synth_a.program_change(ch, 0);
                            let _ = self.synth_a.cc(ch, 7, 100);
                            let _ = self.synth_a.cc(ch, 11, 127);
                            let _ = self.synth_a.cc(ch, 10, 64);
                            let _ = self.synth_a.pitch_bend(ch, 8192);

                            let _ = self.synth_b.cc(ch, 0, 0);
                            let _ = self.synth_b.cc(ch, 32, 0);
                            let _ = self.synth_b.program_change(ch, 0);
                            let _ = self.synth_b.cc(ch, 7, 100);
                            let _ = self.synth_b.cc(ch, 11, 127);
                            let _ = self.synth_b.cc(ch, 10, 64);
                            let _ = self.synth_b.pitch_bend(ch, 8192);
                        }
                    }
                    MidiEngineEvent::MidiPlay {
                        port,
                        channel,
                        kind,
                        is_drum_channel: _,
                    } => {
                        // 포트 A(0번 포트)일 때 뮤트 마스크 필터링 적용 (리듬 변환 모드가 활성화되었을 때만 작동)
                        if self.rhythm_engine.current_rhythm != Rhythm::Original && port == 0 {
                            let ch_bit = 1 << channel;
                            if (self.rhythm_mute_mask & ch_bit) != 0 {
                                continue;
                            }
                        }

                        // 리듬변환 작동 중일 때: 멜로디 채널이 아니면 원곡 이벤트 완전 필터링(무시)
                        let is_melody = self.sequencer.melody_channels.contains(&(port, channel));
                        if self.rhythm_engine.current_rhythm != Rhythm::Original && !is_melody {
                            // 수신 뮤트 필터 마스크가 비어있을 때만 기존 논리대로 멜로디 제외 채널들을 일괄 차단
                            if self.rhythm_mute_mask == 0 {
                                continue;
                            }
                        }

                        // 포트 범위 설정: SMF 포트 번호 그대로 맵핑하되 안전하게 min(1) 적용
                        // 단, Port P 등 15번 포트와 같이 고번호 포트 이진 이벤트는 synth_b 포트(1)로 라우팅되도록 설정하여 소리 및 코드 추출에 기여하게 함
                        let synth_port = if port >= 1 { 1 } else { 0 };
                        let is_drum_ch = self.drum_channels[synth_port][channel as usize];
                        let target_channel = channel as u32;
                        // 포트에 해당되는 synth 지정
                        let synth = if synth_port == 0 { &self.synth_a } else { &self.synth_b };

                        if let midly::TrackEventKind::Midi { message, .. } = kind {
                            match message {
                                midly::MidiMessage::NoteOn { key, vel } => {
                                    let raw_key = key.as_int();
                                    let vel = vel.as_int();

                                    if vel > 0 {
                                        let final_key = if is_drum_ch {
                                            raw_key
                                        } else {
                                            (raw_key as i8 + self.master_key).clamp(0, 127) as u8
                                        };

                                        let p = synth_port;
                                        let ch = channel as usize;
                                        if vel > self.channel_velocities[p][ch] {
                                            self.channel_velocities[p][ch] = vel;
                                        }

                                        self.active_notes.push((synth_port as u8, channel, final_key));
                                        let _ = synth.note_on(target_channel, final_key as u32, vel as u32);
                                    } else {
                                        let final_key = if is_drum_ch {
                                            raw_key
                                        } else {
                                            (raw_key as i8 + self.master_key).clamp(0, 127) as u8
                                        };
                                        self.active_notes.retain(|&(p, ch, n)| {
                                            !(p == synth_port as u8 && ch == channel && n == final_key)
                                        });
                                        let _ = synth.note_off(target_channel, final_key as u32);
                                    }
                                }
                                midly::MidiMessage::NoteOff { key, .. } => {
                                    let raw_key = key.as_int();
                                    let final_key = if is_drum_ch {
                                        raw_key
                                    } else {
                                        (raw_key as i8 + self.master_key).clamp(0, 127) as u8
                                    };
                                    self.active_notes.retain(|&(p, ch, n)| {
                                        !(p == synth_port as u8 && ch == channel && n == final_key)
                                    });
                                    let _ = synth.note_off(target_channel, final_key as u32);
                                }
                                midly::MidiMessage::Controller { controller, value } => {
                                    let cc_num = controller.as_int();
                                    let cc_val = value.as_int();
                                    let p = synth_port;
                                    let ch = channel as usize;

                                    match cc_num {
                                        0 => {
                                            self.bank_msb[p][ch] = cc_val;
                                            if ch != 9 { // 10번 채널은 항상 드럼 유지
                                                if !self.drum_channels[p][ch] {
                                                    if cc_val == 120 || cc_val == 126 || cc_val == 127 {
                                                        self.drum_channels[p][ch] = true;
                                                    } else {
                                                        self.drum_channels[p][ch] = false;
                                                    }
                                                }
                                            }
                                        }
                                        32 => {
                                            self.bank_lsb[p][ch] = cc_val;
                                        }
                                        // RPN/NRPN 상태 추적 (노래방 미디 CC#6 오염 방지)
                                        6 => {
                                            // CC#6 = Data Entry MSB: 현재 RPN/NRPN 컨텍스트에 따라 해석
                                            if self.nrpn_active[p][ch] {
                                                // NRPN 활성 상태 → fluidlite로 전달 (GS 튜닝 등)
                                                let _ = synth.cc(target_channel, cc_num as u32, cc_val as u32);
                                            } else if self.rpn_msb[p][ch] == 0 && self.rpn_lsb[p][ch] == 0 {
                                                // RPN (0,0) = Pitch Bend Range → 전용 API로 직접 설정
                                                let _ = synth.pitch_wheel_sens(target_channel, cc_val as u32);
                                            }
                                            // 그 외 RPN 콤보는 무시 (정의되지 않은 RPN 조합)
                                        }
                                        99 => {
                                            // NRPN MSB → NRPN 모드 활성화
                                            self.nrpn_active[p][ch] = true;
                                            let _ = synth.cc(target_channel, cc_num as u32, cc_val as u32);
                                        }
                                        98 => {
                                            // NRPN LSB
                                            let _ = synth.cc(target_channel, cc_num as u32, cc_val as u32);
                                        }
                                        101 => {
                                            // RPN MSB → RPN 모드 활성화 (NRPN 해제)
                                            self.rpn_msb[p][ch] = cc_val;
                                            self.nrpn_active[p][ch] = false;
                                            let _ = synth.cc(target_channel, cc_num as u32, cc_val as u32);
                                        }
                                        100 => {
                                            // RPN LSB
                                            self.rpn_lsb[p][ch] = cc_val;
                                            self.nrpn_active[p][ch] = false;
                                            let _ = synth.cc(target_channel, cc_num as u32, cc_val as u32);
                                        }
                                        _ => {
                                            let _ = synth.cc(target_channel, cc_num as u32, cc_val as u32);
                                        }
                                    }
                                }
                                midly::MidiMessage::ProgramChange { program } => {
                                    let p = synth_port;
                                    let ch = channel as usize;
                                    let msb = self.bank_msb[p][ch];
                                    let lsb = self.bank_lsb[p][ch];
                                    let prog = program.as_int() as u32;

                                    let is_drum = self.drum_channels[p][ch];

                                    // 드럼 채널일 경우 fluidlite 드럼 뱅크(128)로 맵핑
                                    let resolved_bank: u32 = if is_drum {
                                        128 << 7
                                    } else {
                                        match self.midi_format {
                                            MidiFormat::GM => 0,
                                            MidiFormat::GS => 0,
                                            MidiFormat::XG => (msb as u32) << 7,
                                        }
                                    };

                                    // 드럼 채널은 bank 128로 설정 후 program_change 적용
                                    if is_drum {
                                        let _ = synth.bank_select(target_channel, 128);
                                        let _ = synth.program_change(target_channel, prog);
                                    } else {
                                        let _ = synth.bank_select(target_channel, resolved_bank >> 7);
                                        let _ = synth.cc(target_channel, 32, lsb as u32);
                                        let _ = synth.program_change(target_channel, prog);
                                    }
                                }
                                midly::MidiMessage::PitchBend { bend } => {
                                    let raw = bend.as_int();
                                    let value = (raw as i32 + 8192).clamp(0, 16383) as u32;
                                    let _ = synth.pitch_bend(target_channel, value);
                                }
                                _ => {}
                            }
                        }
                    }
                    MidiEngineEvent::SetDrumChannel { port, channel, is_drum } => {
                        let port = port.min(1) as usize;
                        let channel = channel.min(15) as usize;
                        self.drum_channels[port][channel] = is_drum;
                    }
                    MidiEngineEvent::TempoChange { tempo } => {
                        self.status.lock().unwrap().current_tempo = tempo as i32;
                        let _ = self.ui_tx.send(MidiEngineEvent::TempoChange { tempo });
                    }
                    MidiEngineEvent::RhythmEngineControl { command, mask_lo, mask_hi } => {
                        // 사용자가 수동으로 리듬변환 꺼짐(Original) 모드를 선택해 놓았다면
                        // 미디 내부의 자동 리듬변환 SysEx 제어 시그널은 전면 바이패스(무시) 조치한다
                        if self.user_selected_rhythm == Rhythm::Original {
                            continue;
                        }

                        // 리듬 제어 메세지 처리
                        // command: 0x01 = 리듬 변환 작동, 0x00 = 리듬 변환 해제
                        if command == 0x01 {
                            // 뮤트 마스크에 채널 마킹 적용 (사용자 선택 리듬 가동 시)
                            self.rhythm_mute_mask = ((mask_hi as u16) << 8) | (mask_lo as u16);
                            self.rhythm_engine.current_rhythm = self.user_selected_rhythm;
                        } else if command == 0x00 {
                            self.rhythm_mute_mask = 0; // 마스크 초기화
                            self.rhythm_engine.current_rhythm = Rhythm::Original;
                            
                            // 실시간으로 이전 음 찌꺼기 끄기 (Note Stuck 방지)
                            for port in 0..2 {
                                let synth = if port == 0 { &self.synth_a } else { &self.synth_b };
                                for ch in 0..16 {
                                    let _ = synth.cc(ch as u32, 123, 0); // All Notes Off
                                    let _ = synth.cc(ch as u32, 120, 0); // All Sound Off
                                }
                            }
                            self.active_notes.clear();
                            self.restore_original_states();
                        }
                    }
                    MidiEngineEvent::KeySignature { key, is_sharp } => {
                        // 원곡 키 시그니처를 status에 저장하고 UI에도 전달
                        let mut status = self.status.lock().unwrap();
                        status.song_key_sig = Some((key, is_sharp));
                        let _ = self.ui_tx.send(MidiEngineEvent::KeySignature { key, is_sharp });
                    }
                    other_event => {
                        let _ = self.ui_tx.send(other_event);
                    }
                }
            }

            // [리듬 개입] 생성해 놓은 엇박 리듬 노트들을 현재 진행 틱에 맞춰 분출
            if self.rhythm_engine.current_rhythm != Rhythm::Original {
                let current_tick = self.sequencer.current_tick as u32;
                
                // 디버깅 목적으로 현재 지점(Tick)에 매칭되는 BsChordEvent 정보를 가져와서 
                // ChordUpdate 이벤트를 UI 버스 채널로 실시간 송출
                if let Some(chord) = self.rhythm_engine.get_chord_at_tick(current_tick, &self.sequencer.chord_timeline) {
                    let _ = self.ui_tx.send(MidiEngineEvent::ChordUpdate {
                        root_pitch: chord.root_pitch,
                        is_minor: chord.is_minor,
                        is_7th: chord.is_7th,
                        is_maj7: chord.is_maj7,
                    });
                }

                while self.next_rhythm_note_index < self.generated_rhythm_notes.len() {
                    let note = &self.generated_rhythm_notes[self.next_rhythm_note_index];
                    if note.tick <= current_tick {
                        // 생성된 리듬은 무조건 Synth B 포트에서 소리 출력하도록 매핑 (드럼은 드럼전용, 그외는 댄스 셋)
                        let is_drum = note.channel == 9;
                        
                        // 내장 리듬의 synth_b 독점 전용 우회 채널 매핑 분리 (음색 실종 충돌 버그 영구 박멸!)
                        let target_channel: u32 = if is_drum {
                            15
                        } else if note.channel == 1 {
                            14
                        } else {
                            13
                        };
                        
                        // 볼륨 마스킹 및 전이
                        let vel = note.velocity as usize;
                        let ch_idx = note.channel as usize;
                        if vel as u8 > self.channel_velocities[1][ch_idx] {
                            self.channel_velocities[1][ch_idx] = vel as u8;
                        }

                        // 조옮김(KEY) 적용은 멜로디만 하거나 댄스 리듬에도 통합 이조 적용 결정 가능
                        // (반주는 마스터 키 이조를 같이 타고 가되, 드럼 채널(9)만 이조 무력화)
                        let final_key = if is_drum {
                            note.note_number
                        } else {
                            (note.note_number as i8 + self.master_key).clamp(0, 127) as u8
                        };

                        if note.velocity > 0 {
                            // 같은 채널에서 동일한 음정이 미처 꺼지지 않고 다시 켜지는 경우(NoteOn)
                            // 하드웨어 레벨에서 먼저 강제 NoteOff 처리를 해 주어 잔향 중첩과 스택 과부하를 원천 방지한다
                            let _ = self.synth_b.note_off(target_channel, final_key as u32);
                            self.active_notes.retain(|&(p, ch, n)| {
                                !(p == 1 && ch == target_channel as u8 && n == final_key)
                            });

                            // 동적 노트 추적 등록
                            self.active_notes.push((1, target_channel as u8, final_key));

                            // 해당되는 반주 악기 패치(Program Change) 강제 맵핑 적용
                            if let Some(rhythm_pattern) = self.rhythm_engine.pattern_library.get(&self.rhythm_engine.current_rhythm) {
                                if let Some(track) = rhythm_pattern.tracks.iter().find(|t| {
                                    match t.track_type {
                                        crate::rhythm_engine::TrackType::Drum => is_drum,
                                        crate::rhythm_engine::TrackType::Bass => note.channel == 1,
                                        crate::rhythm_engine::TrackType::Accompaniment => note.channel == 2,
                                    }
                                }) {
                                    if is_drum {
                                        let _ = self.synth_b.bank_select(target_channel, 128);
                                    } else {
                                        let _ = self.synth_b.bank_select(target_channel, 0);
                                    }
                                    let _ = self.synth_b.program_change(target_channel, track.instrument_program as u32);

                                    // 원곡 CC가 내장 리듬의 볼륨/정렬 등을 난도질하지 못하도록 기본 제어값 실시간 강제 고정 락인!
                                    let _ = self.synth_b.cc(target_channel, 7, 100);  // 채널 볼륨 100 고정 강경 탑재
                                    let _ = self.synth_b.cc(target_channel, 11, 127); // Expression 127 고정
                                    let _ = self.synth_b.cc(target_channel, 10, 64);  // Pan Center 고정
                                }
                            }

                            // 리듬 노트 발송
                            let _ = self.synth_b.note_on(target_channel, final_key as u32, note.velocity as u32);
                        } else {
                            // Note-Off (velocity = 0) 처리
                            self.active_notes.retain(|&(p, ch, n)| {
                                !(p == 1 && ch == target_channel as u8 && n == final_key)
                            });
                            let _ = self.synth_b.note_off(target_channel, final_key as u32);
                        }

                        self.next_rhythm_note_index += 1;
                    } else {
                        break;
                    }
                }
            }

            // 4. 오디오 합성
            let mut temp_a = [0.0f32; 2];
            let mut temp_b = [0.0f32; 2];

            let _ = temp_a.write_samples(&self.synth_a);
            let _ = temp_b.write_samples(&self.synth_b);

            output_buffer[sample_idx] = temp_a[0] + temp_b[0];      //L
            output_buffer[sample_idx + 1] = temp_a[1] + temp_b[1];  //R

            sample_idx += 2;

            // 5. 연주가 끝났는지 확인 (모든 이벤트 처리 완료 시)
            if self.sequencer.is_finished() {
                self.current_state = PlayerState::Stopped;

                // 곡 완료 시 원래 사용자가 지정한 수동선택 리듬 복원
                self.rhythm_engine.current_rhythm = self.user_selected_rhythm;
                self.rhythm_mute_mask = 0; // 마스크 초기화

                {
                    // status 가드가 self 메서드 호출 전에 드롭되도록 스코프 처리
                    let mut status = self.status.lock().unwrap();
                    status.state = PlayerState::Stopped;
                    status.current_tick = 0;
                    status.current_time = std::time::Duration::from_secs(0);
                    status.current_rhythm = self.rhythm_engine.current_rhythm; // 복원상태 반영
                }
                self.elapsed_time_sec = 0.0;
                self.all_notes_off();
                self.sequencer.reset();
                self.next_rhythm_note_index = 0; // 연주가 완전히 끝났을 때 엇박 리듬 인덱스도 재생 준비를 위해 0으로 함께 리셋

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

        // 소프트 decay: 버퍼 주기마다 각 채널 velocity를 서서히 감쇠
        for port in 0..2usize {
            for ch in 0..16usize {
                self.channel_velocities[port][ch] =
                    self.channel_velocities[port][ch].saturating_sub(4);
            }
            let _ = self.ui_tx.send(MidiEngineEvent::ChannelLevel {
                port: port as u8,
                levels: self.channel_velocities[port],
            });
        }
    }
}

/// MIMI 엔진 컨텍스트와 핸들을 생성하여 반환 (오디오 백엔드 독립적)
/// 실제 오디오 출력은 반환된 AudioPlaybackContext의 fill_buffer를 통해 상위 레이어가 담당함
pub fn create_mimi_engine(
    sf_path: &str,
    sample_rate: f64,
    mut on_progress: impl FnMut(f32, &str),
) -> Result<(MimiEngineHandle, AudioPlaybackContext), anyhow::Error> {
    on_progress(0.05, "Init Engine...");
    let (command_tx, command_rx) = unbounded::<MimiCommand>();
    let (ui_tx, ui_rx) = unbounded::<MidiEngineEvent>();

    // fluidlite 합성기의 콘솔/stderr 로깅 비활성화 (TUI 화면 깨짐 방지) 및 커스텀 로거 지정
    let ui_tx_log = ui_tx.clone();
    let l_handler = fluidlite::FnLogger::new(move |_lvl, msg| {
        let _ = ui_tx_log.send(MidiEngineEvent::FluidsynthWarning {
            message: msg.to_string(),
        });
    });
    fluidlite::Log::set(
        &[
            fluidlite::LogLevel::Panic,
            fluidlite::LogLevel::Error,
            fluidlite::LogLevel::Warning,
            fluidlite::LogLevel::Info,
            fluidlite::LogLevel::Debug,
        ],
        l_handler,
    );

    // 공유 상태 객체 초기화
    let player_status = Arc::new(Mutex::new(MimiEngineStatus {
        state: PlayerState::Stopped,
        current_tick: 0,
        total_tick: 0,
        current_time: std::time::Duration::from_secs(0),
        tempo: 1.0,
        key: 0,
        volume: 50,
        current_rhythm: Rhythm::Original,
        is_bs_detected: false,
        current_tempo: 500_000,
        song_key_sig: None,
    }));
    let status_clone = Arc::clone(&player_status);

    on_progress(0.15, "Init Synth A...");

    // fluidlite 합성기 설정 (Synth A & B)
    let settings_a = Settings::new()
        .map_err(|e| {
            let msg = format!("FluidLite Settings A creation failed: {:?}", e);
            let _ = ui_tx.send(MidiEngineEvent::FluidsynthWarning { message: msg.clone() });
            anyhow::anyhow!(msg)
        })?;

    if let Some(sr_setting) = settings_a.num("synth.sample-rate") {
        sr_setting.set(sample_rate);
    }

    let synth_a = Synth::new(settings_a)
        .map_err(|e| {
            let msg = format!("FluidLite Synth A creation failed: {:?}", e);
            let _ = ui_tx.send(MidiEngineEvent::FluidsynthWarning { message: msg.clone() });
            anyhow::anyhow!(msg)
        })?;

    on_progress(0.20, "Load Soundfont A...");
    if let Err(e) = synth_a.sfload(sf_path, true) {
        let msg = format!("Load Soundfont A Failed: {:?}", e);
        let _ = ui_tx.send(MidiEngineEvent::FluidsynthWarning { message: msg.clone() });
        return Err(anyhow::anyhow!(msg));
    }

    synth_a.set_gain(1.0);

    on_progress(0.50, "Init Synth B...");

    let settings_b = Settings::new()
        .map_err(|e| {
            let msg = format!("FluidLite Settings B creation failed: {:?}", e);
            let _ = ui_tx.send(MidiEngineEvent::FluidsynthWarning { message: msg.clone() });
            anyhow::anyhow!(msg)
        })?;

    if let Some(sr_setting) = settings_b.num("synth.sample-rate") {
        sr_setting.set(sample_rate);
    }

    let synth_b = Synth::new(settings_b)
        .map_err(|e| {
            let msg = format!("FluidLite Synth B creation failed: {:?}", e);
            let _ = ui_tx.send(MidiEngineEvent::FluidsynthWarning { message: msg.clone() });
            anyhow::anyhow!(msg)
        })?;

    on_progress(0.55, "Load Soundfont B...");
    if let Err(e) = synth_b.sfload(sf_path, true) {
        let msg = format!("Load Soundfont B Failed: {:?}", e);
        let _ = ui_tx.send(MidiEngineEvent::FluidsynthWarning { message: msg.clone() });
        return Err(anyhow::anyhow!(msg));
    }

    synth_b.set_gain(1.0);

    on_progress(0.85, "Init Sequencer...");

    let sequencer = MimiSequencer::empty();
    let sequencer_format = sequencer.format;

    player_status.lock().unwrap().total_tick = 0;

    on_progress(0.95, "Init Playback Context...");

    let playback_context = AudioPlaybackContext {
        sequencer,
        synth_a,
        synth_b,
        command_rx,
        ui_tx: ui_tx.clone(),
        status: status_clone,
        current_state: PlayerState::Stopped,
        master_key: 0,
        tempo_scale: 1.0,
        master_volume: 50,
        sample_rate,
        active_notes: Vec::new(),
        elapsed_time_sec: 0.0,
        midi_format: sequencer_format,
        bank_msb: [[0u8; 16]; 2],
        bank_lsb: [[0u8; 16]; 2],
        drum_channels: {
            let mut dc = [[false; 16]; 2];
            dc[0][9] = true;
            dc[1][9] = true;
            dc[1][15] = true; // 15번 체널 드럼 등록 (리듬 엔진 전용 우회 채널)
            dc
        },
        channel_velocities: [[0u8; 16]; 2],
        rpn_msb: [[127u8; 16]; 2],
        rpn_lsb: [[127u8; 16]; 2],
        nrpn_active: [[false; 16]; 2],
        rhythm_engine: RhythmEngine::new(Rhythm::Original),
        generated_rhythm_notes: Vec::new(),
        next_rhythm_note_index: 0,
        rhythm_mute_mask: 0,
        user_selected_rhythm: Rhythm::Original,
    };

    on_progress(1.0, "Engine Initialized!");

    let handle = MimiEngineHandle {
        command_tx,
        status: player_status,
        ui_rx,
        ui_tx,
    };

    Ok((handle, playback_context))
}

pub fn get_engine_info() -> MimiEngineInfo {
    MimiEngineInfo {
        name: "MimiEngine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        author: env!("CARGO_PKG_AUTHORS").to_string().split(',').next().unwrap().to_string(),
        license: env!("CARGO_PKG_LICENSE").to_string(),
    }
}