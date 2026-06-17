// hibiki.rs - Hibiki 사운드폰트 엔진 (자체 신디사이저)
// vendored sf2_oxi의 SoundFont2 파서와 자체 voice.rs / dsp.rs를 통합한
// OxiSynth 비의존 신디사이저. mimi_core의 API는 그대로 유지.

pub mod sf2_oxi;
pub mod voice;
pub mod dsp;

use std::collections::HashMap;
use std::sync::Arc;

use sf2_oxi::adapter::{Instrument, InstrumentZone, PresetZone, Sample, Sf2File};
use voice::{Voice, VoiceManager, VoiceState};
use dsp::{
    Celeste, Chorus as DspChorus, Delay as DspDelay, Phaser, Reverb as DspReverb, Tremolo,
};

// 외부 호환성을 위한 sf2 모듈 (예전 OxiSynth 호환)
pub mod sf2 {
    pub use crate::sf2_oxi::*;
    pub use crate::sf2_oxi::adapter::{
        Instrument, InstrumentZone, Preset, PresetZone, Sample, SampleType, Sf2File,
    };
}

/// 호환용 SoundFontId 별칭 (OxiSynth와 같은 이름 유지)
pub type SoundFontId = u32;

/// 미디 이벤트 (OxiSynth 의존 제거를 위해 자체 정의)
#[derive(Debug, Clone, Copy)]
pub enum MidiEvent {
    NoteOn { channel: u8, key: u8, vel: u8 },
    NoteOff { channel: u8, key: u8 },
    ControlChange { channel: u8, ctrl: u8, value: u8 },
    ProgramChange { channel: u8, program_id: u8 },
    PitchBend { channel: u8, value: u16 },
}

/// Hibiki 엔진 설정
pub struct HibikiSettings {
    sample_rate: f64,
    max_voices: usize,
    gain: f32,
}

impl HibikiSettings {
    pub fn new() -> Self {
        Self {
            sample_rate: 44100.0,
            max_voices: 256,
            gain: 0.4,
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn set_sample_rate(&mut self, rate: f64) {
        self.sample_rate = rate;
    }

    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    pub fn set_max_voices(&mut self, voices: usize) {
        self.max_voices = voices;
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 10.0);
    }
}

impl Default for HibikiSettings {
    fn default() -> Self {
        Self::new()
    }
}

/// 사운드폰트 정보 (mimi_core 호환)
#[derive(Debug, Clone, Default)]
pub struct SoundFontInfo {
    pub samples: Vec<Sample>,
    pub instruments: Vec<Instrument>,
    pub presets: Vec<crate::sf2_oxi::adapter::Preset>,
    pub smpl_data: Arc<Vec<i16>>,
    pub name: String,
}

/// Hibiki Logger (API 호환용)
pub struct HibikiLogger;

impl HibikiLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HibikiLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// 로그 레벨
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    None = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

static CURRENT_LOG_LEVEL: std::sync::Mutex<LogLevel> = std::sync::Mutex::new(LogLevel::Warn);

/// 로그 레벨 설정
pub fn log_level(level: LogLevel) {
    *CURRENT_LOG_LEVEL.lock().unwrap() = level;
}

/// 로그 레벨들을 한 번에 설정 (API 호환)
pub fn set_log_levels(_synth: &HibikiSynth, synth_level: LogLevel, _voice_level: LogLevel) {
    *CURRENT_LOG_LEVEL.lock().unwrap() = synth_level;
}

/// 전역 로그 레벨 설정
pub fn set_log_levels_global(synth_level: LogLevel, _voice_level: LogLevel) {
    *CURRENT_LOG_LEVEL.lock().unwrap() = synth_level;
}

/// 미디 채널 상태 (CC, RPN/NRPN, bank, program, pitch bend 등)
#[derive(Debug, Clone)]
pub struct ChannelState {
    /// Bank Select MSB (CC#0)
    pub bank_msb: u8,
    /// Bank Select LSB (CC#32)
    pub bank_lsb: u8,
    /// Program Number
    pub program: u8,
    /// 드럼 채널 여부
    pub is_drum: bool,
    /// 채널 볼륨 (CC#7, 0~127)
    pub volume: u8,
    /// 패닝 (CC#10, 0~127, 64 = center)
    pub pan: u8,
    /// 익스프레션 (CC#11, 0~127)
    pub expression: u8,
    /// 피치벤드 (0~16383, center 8192)
    pub pitch_bend: u16,
    /// 피치 휠 감도 (RPN 0,0 결과, 반음 단위)
    pub pitch_bend_range: u8,
    /// 모듈레이션 (CC#1, 0~127)
    pub modulation: u8,
    /// 필터 컷오프 (CC#74, 0~127)
    pub cutoff: u8,
    /// 필터 공진 (CC#71, 0~127)
    pub resonance: u8,
    /// 비브라토 깊이 (CC#76, 0~127)
    pub vibrato_depth: u8,
    /// 비브라토 레이트 (Hz, CC#77에 매핑)
    pub vibrato_rate: u8,
    /// 어택 타임 (CC#73)
    pub attack_time: u8,
    /// 디케이 타임 (CC#75)
    pub decay_time: u8,
    /// 브라이트니스 (CC#72)
    pub brightness: u8,
    /// 이펙트 send
    pub reverb_send: u8,    // CC#91
    pub chorus_send: u8,    // CC#93
    pub delay_send: u8,     // CC#94
    pub tremolo_send: u8,   // CC#92
    pub phaser_send: u8,    // CC#95
    pub celeste_send: u8,   // (CC#96 등)
    /// RPN/NRPN 추적
    pub rpn_msb: u8,
    pub rpn_lsb: u8,
    pub nrpn_active: bool,
    /// 페달
    pub sustain_pedal: bool, // CC#64
    pub sostenuto: bool,    // CC#66
    pub soft_pedal: bool,    // CC#67
    pub hold2: bool,         // CC#69
}

impl ChannelState {
    fn new() -> Self {
        Self {
            bank_msb: 0,
            bank_lsb: 0,
            program: 0,
            is_drum: false,
            volume: 100,
            pan: 64,
            expression: 127,
            pitch_bend: 8192,
            pitch_bend_range: 2,
            modulation: 0,
            cutoff: 127,
            resonance: 0,
            vibrato_depth: 0,
            vibrato_rate: 0,
            attack_time: 64,
            decay_time: 64,
            brightness: 64,
            reverb_send: 40,
            chorus_send: 0,
            delay_send: 0,
            tremolo_send: 0,
            phaser_send: 0,
            celeste_send: 0,
            rpn_msb: 127,
            rpn_lsb: 127,
            nrpn_active: false,
            sustain_pedal: false,
            sostenuto: false,
            soft_pedal: false,
            hold2: false,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    /// 정규화된 패닝 (-1.0: 왼쪽, 0.0: 중앙, 1.0: 오른쪽)
    fn pan_normalized(&self) -> f32 {
        (self.pan as f32 - 64.0) / 63.0
    }

    /// 정규화된 볼륨 (0.0~1.0)
    fn volume_normalized(&self) -> f32 {
        (self.volume as f32 / 127.0) * (self.expression as f32 / 127.0)
    }

    /// 피치벤드를 cents로 변환
    fn pitch_bend_cents(&self) -> f32 {
        let bend = (self.pitch_bend as i32 - 8192) as f32;
        bend / 8192.0 * (self.pitch_bend_range as f32 * 100.0)
    }
}

/// 이펙트 체인
struct SynthEffects {
    reverb: DspReverb,
    chorus: DspChorus,
    delay: DspDelay,
    tremolo: Tremolo,
    phaser: Phaser,
    celeste: Celeste,
    reverb_enabled: bool,
    chorus_enabled: bool,
    delay_enabled: bool,
    tremolo_enabled: bool,
    phaser_enabled: bool,
    celeste_enabled: bool,
    /// 마스터 wet/dry 비율
    reverb_wet: f32,
    chorus_wet: f32,
    delay_wet: f32,
}

impl SynthEffects {
    fn new(sample_rate: f32) -> Self {
        Self {
            reverb: DspReverb::new(sample_rate),
            chorus: DspChorus::new(sample_rate),
            delay: DspDelay::new(sample_rate),
            tremolo: Tremolo::new(sample_rate),
            phaser: Phaser::new(sample_rate),
            celeste: Celeste::new(sample_rate),
            reverb_enabled: true,
            chorus_enabled: true,
            delay_enabled: false,
            tremolo_enabled: false,
            phaser_enabled: false,
            celeste_enabled: false,
            reverb_wet: 0.3,
            chorus_wet: 0.5,
            delay_wet: 0.4,
        }
    }
}

/// Hibiki 사운드폰트 신디사이저
/// 자체 SF2 파서 + voice 합성 + 이펙트 체인 통합
pub struct HibikiSynth {
    settings: HibikiSettings,
    /// 로드된 사운드폰트
    sf2: Option<Sf2File>,
    /// 16개 MIDI 채널 상태
    channels: [ChannelState; 16],
    /// 단일 보이스 풀
    voice_manager: VoiceManager,
    /// 활성 노트 추적: (channel, key) -> voice index in voice_manager.voices
    active_notes: HashMap<(u8, u8), usize>,
    /// 채널별 키 홀드 (hold2 등)로 보이스가 release 되지 않은 노트
    held_notes: HashMap<(u8, u8), usize>,
    /// 이펙트 체인
    effects: SynthEffects,
    /// 샘플 레이트
    sample_rate: f32,
    /// 마스터 게인
    gain: f32,
}

impl HibikiSynth {
    pub fn new(settings: HibikiSettings) -> Result<Self, String> {
        let sample_rate = settings.sample_rate as f32;
        Ok(Self {
            settings: HibikiSettings {
                sample_rate: settings.sample_rate,
                max_voices: settings.max_voices,
                gain: settings.gain,
            },
            sf2: None,
            channels: [
                ChannelState::new(), ChannelState::new(), ChannelState::new(), ChannelState::new(),
                ChannelState::new(), ChannelState::new(), ChannelState::new(), ChannelState::new(),
                ChannelState::new(), ChannelState::new(), ChannelState::new(), ChannelState::new(),
                ChannelState::new(), ChannelState::new(), ChannelState::new(), ChannelState::new(),
            ],
            voice_manager: VoiceManager::new(settings.max_voices),
            active_notes: HashMap::new(),
            held_notes: HashMap::new(),
            effects: SynthEffects::new(sample_rate),
            sample_rate,
            gain: settings.gain,
        })
    }

    /// 사운드폰트 로드 (sf2_oxi 기반 자체 파싱)
    pub fn sfload(&mut self, path: &str, _reset_presets: bool) -> Result<u32, String> {
        let sf = Sf2File::from_path(path)?;
        let preset_count = sf.presets.len() as u32;
        self.sf2 = Some(sf);
        Ok(preset_count)
    }

    /// 사운드폰트 정보 가져오기
    pub fn get_soundfont_info(&self) -> Option<SoundFontInfo> {
        self.sf2.as_ref().map(|sf| SoundFontInfo {
            samples: sf.samples.clone(),
            instruments: sf.instruments.clone(),
            presets: sf.presets.clone(),
            smpl_data: sf.smpl_data.clone(),
            name: sf.name.clone(),
        })
    }

    /// 게인 설정
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 10.0);
    }

    /// 게인 가져오기
    pub fn get_gain(&self) -> f32 {
        self.gain
    }

    /// MIDI 이벤트를 자체 큐에 적재
    pub fn send_event(&mut self, event: MidiEvent) -> Result<(), String> {
        match event {
            MidiEvent::NoteOn { channel, key, vel } => self.note_on(channel as u32, key as u32, vel as u32),
            MidiEvent::NoteOff { channel, key } => self.note_off(channel as u32, key as u32),
            MidiEvent::ControlChange { channel, ctrl, value } => {
                self.cc(channel as u32, ctrl as u32, value as u32)
            }
            MidiEvent::ProgramChange { channel, program_id } => {
                self.program_change(channel as u32, program_id as u32)
            }
            MidiEvent::PitchBend { channel, value } => self.pitch_bend(channel as u32, value as u32),
        }
    }

    /// 노트 온
    pub fn note_on(&mut self, channel: u32, key: u32, vel: u32) -> Result<(), String> {
        if vel == 0 {
            return self.note_off(channel, key);
        }
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }

        // 드럼 채널이고 키가 이미 활성하면 같은 voice 재활용
        let lookup = (ch as u8, key as u8);
        if let Some(&vidx) = self.active_notes.get(&lookup) {
            if let Some(v) = self.voice_manager.voices.get_mut(vidx) {
                v.release();
            }
        }

        // SF2에서 적절한 preset/instrument/zone/sample을 찾는다
        let (sample, _instrument, attack, decay, sustain, release, attenuation) = {
            let sf = match self.sf2.as_ref() {
                Some(sf) => sf,
                None => return Err("no soundfont loaded".to_string()),
            };
            find_voice_params(sf, &self.channels[ch], key as u8, vel as u8)
                .ok_or_else(|| "no matching instrument zone".to_string())?
        };

        // 보이스 할당
        let vidx = {
            let voices = &mut self.voice_manager.voices;
            // 비활성 보이스 찾기
            let mut idx = None;
            for (i, v) in voices.iter().enumerate() {
                if v.state == VoiceState::Off {
                    idx = Some(i);
                    break;
                }
            }
            let idx = match idx {
                Some(i) => i,
                None => {
                    // 풀이 가득 차면 가장 오래된 sustain 보이스를 release
                    let mut oldest = None;
                    for (i, v) in voices.iter().enumerate() {
                        if v.state == VoiceState::Sustain || v.state == VoiceState::Decay {
                            oldest = Some(i);
                            break;
                        }
                    }
                    match oldest {
                        Some(i) => {
                            voices[i].release();
                            i
                        }
                        None => {
                            // 풀이 가득 차고 모든 보이스가 release 중이면 가장 오래된 것 steal
                            voices[0].release();
                            0
                        }
                    }
                }
            };
            idx
        };

        // 보이스 트리거
        let smpl_data = {
            let sf = self.sf2.as_ref().unwrap();
            sf.smpl_data.clone()
        };
        {
            let voice = &mut self.voice_manager.voices[vidx];
            voice.trigger(
                &sample,
                smpl_data,
                key as u8,
                vel as u8,
                self.sample_rate,
                attack,
                decay,
                sustain,
                release,
                attenuation,
                ch as u8,
            );
            // 채널 상태 반영
            let ch_state = &self.channels[ch];
            // 패닝
            voice.pan = ch_state.pan_normalized() * 0.5 + 0.5;
            // 모듈레이션
            voice.set_modulation_depth(ch_state.modulation as f32 / 127.0);
            // 필터
            voice.set_filter(ch_state.cutoff as f32, ch_state.resonance as f32);
            // 비브라토
            voice.set_vibrato_depth(ch_state.vibrato_depth as f32);
            // 비브라토 레이트 (CC77: 0~127 -> 0.1~8Hz 정도)
            let vib_rate = 0.5 + (ch_state.vibrato_rate as f32 / 127.0) * 7.5;
            voice.set_vibrato_rate(vib_rate);
            // 피치벤드
            let bend = ch_state.pitch_bend as f32;
            voice.apply_pitch_bend(bend - 8192.0, ch_state.pitch_bend_range as f32);
        }

        self.active_notes.insert(lookup, vidx);
        Ok(())
    }

    /// 노트 오프
    pub fn note_off(&mut self, channel: u32, key: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        let lookup = (ch as u8, key as u8);

        // sustain pedal down 이면 즉시 release 하지 않고 held_notes로 이동
        if self.channels[ch].sustain_pedal || self.channels[ch].sostenuto {
            if let Some(&vidx) = self.active_notes.get(&lookup) {
                self.held_notes.insert(lookup, vidx);
            }
            return Ok(());
        }

        if let Some(&vidx) = self.active_notes.get(&lookup) {
            if let Some(v) = self.voice_manager.voices.get_mut(vidx) {
                v.release();
            }
            self.active_notes.remove(&lookup);
        }
        Ok(())
    }

    /// 컨트롤 체인지
    pub fn cc(&mut self, channel: u32, control: u32, value: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        let ctrl = control as u8;
        let val = value as u8;
        let ch_state = &mut self.channels[ch];

        match ctrl {
            0 => ch_state.bank_msb = val,
            1 => {
                ch_state.modulation = val;
                // 활성 보이스에 모듈레이션 깊이 적용
                let depth = val as f32 / 127.0;
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch && v.state != VoiceState::Off {
                        v.set_modulation_depth(depth);
                    }
                }
            }
            7 => ch_state.volume = val,
            10 => ch_state.pan = val,
            11 => ch_state.expression = val,
            32 => ch_state.bank_lsb = val,
            64 => ch_state.sustain_pedal = val >= 64,
            66 => {
                let new_state = val >= 64;
                ch_state.sostenuto = new_state;
                if !new_state {
                    // sostenuto off -> held 노트 release
                    let held = std::mem::take(&mut self.held_notes);
                    for ((c, k), vidx) in held {
                        if c as usize == ch {
                            if let Some(v) = self.voice_manager.voices.get_mut(vidx) {
                                v.release();
                            }
                            self.active_notes.remove(&(c, k));
                        } else {
                            // 다른 채널이므로 다시 held에 넣기
                            self.held_notes.insert((c, k), vidx);
                        }
                    }
                }
            }
            67 => ch_state.soft_pedal = val >= 64,
            69 => ch_state.hold2 = val >= 64,
            71 => {
                ch_state.resonance = val;
                let cutoff = ch_state.cutoff as f32;
                let resonance = val as f32;
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch && v.state != VoiceState::Off {
                        v.set_filter(cutoff, resonance);
                    }
                }
            }
            72 => ch_state.brightness = val,
            73 => {
                ch_state.attack_time = val;
                // 어택 타임 변경 시 미래 노트부터 적용 (활성 보이스에는 적용 안 함)
            }
            74 => {
                ch_state.cutoff = val;
                let cutoff = val as f32;
                let resonance = ch_state.resonance as f32;
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch && v.state != VoiceState::Off {
                        v.set_filter(cutoff, resonance);
                    }
                }
            }
            75 => ch_state.decay_time = val,
            76 => {
                ch_state.vibrato_depth = val;
                let d = val as f32;
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch && v.state != VoiceState::Off {
                        v.set_vibrato_depth(d);
                    }
                }
            }
            77 => {
                ch_state.vibrato_rate = val;
                let rate = 0.5 + (val as f32 / 127.0) * 7.5;
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch && v.state != VoiceState::Off {
                        v.set_vibrato_rate(rate);
                    }
                }
            }
            91 => ch_state.reverb_send = val,
            92 => ch_state.tremolo_send = val,
            93 => ch_state.chorus_send = val,
            94 => ch_state.delay_send = val,
            95 => ch_state.phaser_send = val,
            96 => ch_state.celeste_send = val,
            98 => ch_state.rpn_lsb = val,
            99 => {
                ch_state.nrpn_active = true;
            }
            100 => {
                ch_state.rpn_lsb = val;
                ch_state.nrpn_active = false;
            }
            101 => {
                ch_state.rpn_msb = val;
                ch_state.nrpn_active = false;
            }
            120 => {
                // All Sound Off
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch {
                        v.release();
                    }
                }
                self.active_notes.retain(|&(c, _), _| c as usize != ch);
            }
            121 => {
                // Reset All Controllers
                self.cc_reset(channel)?;
            }
            123 => {
                // All Notes Off (release만, sustain pedal 무시)
                let v_man = &mut self.voice_manager;
                for v in v_man.voices.iter_mut() {
                    if v.channel as usize == ch {
                        v.release();
                    }
                }
                self.active_notes.retain(|&(c, _), _| c as usize != ch);
                self.held_notes.retain(|&(c, _), _| c as usize != ch);
            }
            6 => {
                // Data Entry - RPN 0,0 (Pitch Bend Sensitivity)
                if !ch_state.nrpn_active
                    && ch_state.rpn_msb == 0
                    && ch_state.rpn_lsb == 0
                {
                    ch_state.pitch_bend_range = val;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// 프로그램 체인지
    pub fn program_change(&mut self, channel: u32, program: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        self.channels[ch].program = program as u8;
        Ok(())
    }

    /// 피치벤드
    pub fn pitch_bend(&mut self, channel: u32, value: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        let val = (value as i32).clamp(0, 16383) as u16;
        self.channels[ch].pitch_bend = val;
        // 활성 보이스에 피치벤드 적용
        let bend = val as f32;
        let range = self.channels[ch].pitch_bend_range as f32;
        let v_man = &mut self.voice_manager;
        for v in v_man.voices.iter_mut() {
            if v.channel as usize == ch && v.state != VoiceState::Off {
                v.apply_pitch_bend(bend - 8192.0, range);
            }
        }
        Ok(())
    }

    /// 오디오 출력 (스테레오 1샘플)
    pub fn write_samples(&mut self, output: &mut [f32; 2]) -> Result<(), String> {
        self.render_sample(output, false)
    }

    /// 이펙트 포함 출력 (자체 DSP 이펙트 체인을 거친 출력)
    pub fn write_samples_with_effects(&mut self, output: &mut [f32; 2]) -> Result<(), String> {
        self.render_sample(output, true)
    }

    /// 내부 렌더 함수
    fn render_sample(&mut self, output: &mut [f32; 2], use_effects: bool) -> Result<(), String> {
        let mut dry_l = 0.0f32;
        let mut dry_r = 0.0f32;
        // 이펙트 별 누적 버퍼
        let mut rev_l = 0.0f32;
        let mut rev_r = 0.0f32;
        let mut cho_l = 0.0f32;
        let mut cho_r = 0.0f32;
        let mut del_l = 0.0f32;
        let mut del_r = 0.0f32;
        let mut tre_l = 0.0f32;
        let mut tre_r = 0.0f32;
        let mut pha_l = 0.0f32;
        let mut pha_r = 0.0f32;
        let mut cel_l = 0.0f32;
        let mut cel_r = 0.0f32;

        // 보이스 순회
        let voice_count = self.voice_manager.voices.len();
        for i in 0..voice_count {
            let v = &mut self.voice_manager.voices[i];
            if v.state == VoiceState::Off {
                continue;
            }
            let (mut l, mut r) = v.render_sample();
            // 채널별 dry/이펙트 send 적용
            let ch_idx = v.channel as usize;
            if ch_idx >= 16 {
                continue;
            }
            let ch_state = &self.channels[ch_idx];
            let vol = ch_state.volume_normalized();
            l *= vol;
            r *= vol;
            // dry는 pan 적용
            let pan = ch_state.pan_normalized();
            let (dl, dr) = apply_pan(l, r, pan);
            dry_l += dl;
            dry_r += dr;

            if use_effects {
                // reverb send
                let rev_amt = ch_state.reverb_send as f32 / 127.0;
                rev_l += dl * rev_amt;
                rev_r += dr * rev_amt;
                // chorus send
                let cho_amt = ch_state.chorus_send as f32 / 127.0;
                cho_l += dl * cho_amt;
                cho_r += dr * cho_amt;
                // delay send
                let del_amt = ch_state.delay_send as f32 / 127.0;
                del_l += dl * del_amt;
                del_r += dr * del_amt;
                // tremolo send
                let tre_amt = ch_state.tremolo_send as f32 / 127.0;
                tre_l += dl * tre_amt;
                tre_r += dr * tre_amt;
                // phaser send
                let pha_amt = ch_state.phaser_send as f32 / 127.0;
                pha_l += dl * pha_amt;
                pha_r += dr * pha_amt;
                // celeste send
                let cel_amt = ch_state.celeste_send as f32 / 127.0;
                cel_l += dl * cel_amt;
                cel_r += dr * cel_amt;
            }
        }

        // off된 voice 정리 (active_notes에서도 제거)
        let mut to_remove = Vec::new();
        for i in 0..self.voice_manager.voices.len() {
            if self.voice_manager.voices[i].state == VoiceState::Off {
                to_remove.push(i);
            }
        }
        for i in to_remove {
            self.active_notes.retain(|_, vidx| *vidx != i);
        }

        // 이펙트 처리
        if use_effects {
            // reverb
            if self.effects.reverb_enabled && (rev_l.abs() > 0.0 || rev_r.abs() > 0.0) {
                let (rl, rr) = self.effects.reverb.process_stereo(rev_l, rev_r);
                dry_l += rl * self.effects.reverb_wet;
                dry_r += rr * self.effects.reverb_wet;
            }
            // chorus
            if self.effects.chorus_enabled && (cho_l.abs() > 0.0 || cho_r.abs() > 0.0) {
                let (cl, cr) = self.effects.chorus.process_stereo(cho_l, cho_r);
                dry_l += cl * self.effects.chorus_wet;
                dry_r += cr * self.effects.chorus_wet;
            }
            // delay
            if self.effects.delay_enabled && (del_l.abs() > 0.0 || del_r.abs() > 0.0) {
                let (dl, dr) = self.effects.delay.process_stereo(del_l, del_r);
                dry_l += dl * self.effects.delay_wet;
                dry_r += dr * self.effects.delay_wet;
            }
            // tremolo (모노 진폭 변조)
            if self.effects.tremolo_enabled && (tre_l.abs() > 0.0 || tre_r.abs() > 0.0) {
                let m = self.effects.tremolo.process((tre_l + tre_r) * 0.5);
                dry_l += m * 0.5;
                dry_r += m * 0.5;
            }
            // phaser (모노 위상 변조)
            if self.effects.phaser_enabled && (pha_l.abs() > 0.0 || pha_r.abs() > 0.0) {
                let m = self.effects.phaser.process((pha_l + pha_r) * 0.5);
                dry_l += m * 0.5;
                dry_r += m * 0.5;
            }
            // celeste (디튠 합성)
            if self.effects.celeste_enabled && (cel_l.abs() > 0.0 || cel_r.abs() > 0.0) {
                let d = self.effects.celeste.process((cel_l + cel_r) * 0.5);
                dry_l += d * 0.5;
                dry_r += d * 0.5;
            }
        }

        // 마스터 게인
        output[0] = dry_l * self.gain;
        output[1] = dry_r * self.gain;
        Ok(())
    }

    /// 이펙트 설정 (호환성 - noop)
    pub fn set_effect_level(&self, _reverb: u8, _chorus: u8) {}

    /// 모든 이펙트 활성화 (mimi_core 호환)
    pub fn enable_effect(&mut self, reverb: bool, chorus: bool, _phaser: bool) {
        self.effects.reverb_enabled = reverb;
        self.effects.chorus_enabled = chorus;
    }

    /// 모든 노트 끄기
    pub fn all_notes_off(&mut self) {
        for ch in 0u32..16 {
            let _ = self.cc(ch, 123, 0);
        }
    }

    /// 시스템 리셋
    pub fn system_reset(&mut self) {
        self.all_notes_off();
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
        self.active_notes.clear();
        self.held_notes.clear();
    }

    /// Bank select
    pub fn bank_select(&mut self, channel: u32, bank: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        // bank 128 -> 드럼 채널
        if bank == 128 {
            self.channels[ch].is_drum = true;
        } else {
            self.channels[ch].is_drum = false;
            self.channels[ch].bank_msb = (bank & 0x7F) as u8;
        }
        Ok(())
    }

    /// Pitch wheel sensitivity
    pub fn pitch_wheel_sens(&mut self, channel: u32, value: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        self.channels[ch].pitch_bend_range = (value & 0xFF) as u8;
        Ok(())
    }

    /// Tremolo 활성화
    pub fn enable_tremolo(&mut self, enable: bool) {
        self.effects.tremolo_enabled = enable;
    }
    /// Celeste 활성화
    pub fn enable_celeste(&mut self, enable: bool) {
        self.effects.celeste_enabled = enable;
    }
    /// Phaser 활성화
    pub fn enable_phaser(&mut self, enable: bool) {
        self.effects.phaser_enabled = enable;
    }

    /// Chorus 파라미터
    pub fn set_chorus_params(&mut self, rate: f32, depth: f32, feedback: f32) {
        self.effects.chorus.set_params(rate, depth, feedback);
    }
    /// Reverb 파라미터 (room: 0.0~1.0, damping: 0.0~1.0)
    pub fn set_reverb_params(&mut self, room: f32, damping: f32) {
        self.effects.reverb.set_params(room, damping);
        self.effects.reverb_wet = 0.2 + room * 0.4;
    }
    /// Tremolo 파라미터
    pub fn set_tremolo_params(&mut self, rate: f32, depth: f32) {
        self.effects.tremolo.set_params(rate, depth);
    }
    /// Phaser 파라미터
    pub fn set_phaser_params(&mut self, rate: f32, feedback: f32, depth: f32) {
        self.effects.phaser.set_params(rate, depth, feedback);
    }
    /// Celeste detune
    pub fn set_celeste_detune(&mut self, cents: f32) {
        self.effects.celeste.set_detune(cents);
    }
    /// Delay 활성화
    pub fn enable_delay(&mut self, enable: bool) {
        self.effects.delay_enabled = enable;
    }

    /// 채널 리셋 (Reset All Controllers)
    pub fn cc_reset(&mut self, channel: u32) -> Result<(), String> {
        let ch = channel as usize;
        if ch >= 16 {
            return Err(format!("invalid channel {}", channel));
        }
        // bank, program, pitch_bend는 그대로 두고 CC만 리셋
        let ch_state = &mut self.channels[ch];
        ch_state.modulation = 0;
        ch_state.volume = 100;
        ch_state.pan = 64;
        ch_state.expression = 127;
        ch_state.cutoff = 127;
        ch_state.resonance = 0;
        ch_state.vibrato_depth = 0;
        ch_state.vibrato_rate = 0;
        ch_state.attack_time = 64;
        ch_state.decay_time = 64;
        ch_state.brightness = 64;
        ch_state.reverb_send = 40;
        ch_state.chorus_send = 0;
        ch_state.delay_send = 0;
        ch_state.tremolo_send = 0;
        ch_state.phaser_send = 0;
        ch_state.celeste_send = 0;
        ch_state.sustain_pedal = false;
        ch_state.sostenuto = false;
        ch_state.soft_pedal = false;
        ch_state.hold2 = false;
        ch_state.pitch_bend = 8192;
        ch_state.pitch_bend_range = 2;
        ch_state.rpn_msb = 127;
        ch_state.rpn_lsb = 127;
        ch_state.nrpn_active = false;
        Ok(())
    }

    /// 활성 보이스 수
    pub fn active_voices(&self) -> usize {
        self.voice_manager.active_count()
    }
}

/// 패닝 적용 (-1.0: 왼쪽, 0.0: 중앙, 1.0: 오른쪽)
fn apply_pan(l: f32, r: f32, pan: f32) -> (f32, f32) {
    // 등가 패닝 (sqrt 곡선으로 에너지 보존)
    let pan_norm = (pan + 1.0) * 0.5; // 0.0~1.0
    let pan_l = (std::f32::consts::PI * 0.5 * (1.0 - pan_norm)).cos();
    let pan_r = (std::f32::consts::PI * 0.5 * pan_norm).cos();
    (l * pan_l, r * pan_r)
}

/// 채널의 bank/program/key/velocity로 적절한 sample + envelope 추출
fn find_voice_params(
    sf: &Sf2File,
    ch_state: &ChannelState,
    key: u8,
    velocity: u8,
) -> Option<(Sample, InstrumentZone, f32, f32, f32, f32, f32)> {
    // bank 결정: 드럼은 128, 그 외는 bank_msb
    let bank: u16 = if ch_state.is_drum { 128 } else { ch_state.bank_msb as u16 };
    let program = ch_state.program as u16;

    // preset 검색
    let preset = sf
        .presets
        .iter()
        .find(|p| p.bank == bank && p.preset_num == program)
        .or_else(|| {
            // 정확히 없으면 bank 0에서 찾기
            sf.presets
                .iter()
                .find(|p| p.bank == 0 && p.preset_num == program)
        })?;

    // preset zone 중 key/vel 범위에 맞는 것 찾기
    // global zone (instrument_index가 None인 zone) 제외
    let preset_zone: &PresetZone = preset
        .zones
        .iter()
        .filter(|z| z.instrument_index.is_some())
        .find(|z| {
            let (lo, hi) = z.key_range;
            key >= lo && key <= hi && velocity >= z.velocity_range.0 && velocity <= z.velocity_range.1
        })
        .or_else(|| {
            // 정확히 매치 안 되면 첫 번째 instrument zone
            preset.zones.iter().find(|z| z.instrument_index.is_some())
        })?;

    // instrument 찾기
    let inst_idx = preset_zone.instrument_index?;
    let instrument = sf.instruments.get(inst_idx)?;

    // instrument zone 중 key/vel 범위에 맞는 것 찾기
    // global zone (sample_index가 None) 제외
    let inst_zone = instrument
        .zones
        .iter()
        .filter(|z| z.sample_index.is_some())
        .find(|z| {
            let (lo, hi) = z.key_range;
            key >= lo && key <= hi && velocity >= z.velocity_range.0 && velocity <= z.velocity_range.1
        })
        .or_else(|| {
            instrument.zones.iter().find(|z| z.sample_index.is_some())
        })?;

    // sample 찾기
    let sample_idx = inst_zone.sample_index?;
    let sample = sf.samples.get(sample_idx)?;

    Some((
        sample.clone(),
        inst_zone.clone(),
        inst_zone.attack,
        inst_zone.decay,
        inst_zone.sustain,
        inst_zone.release,
        inst_zone.attenuation,
    ))
}
