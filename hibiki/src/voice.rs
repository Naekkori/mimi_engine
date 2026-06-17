// voice.rs - 보이스 시스템 및 DSP

use std::sync::Arc;

use crate::sf2_oxi::adapter::Sample;
use crate::dsp::{BiquadFilter, LFO};

/// 보이스 상태
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceState {
    /// 음이 꺼진 상태
    Off,
    /// 음이 켜지는 중 (Attack)
    Attack,
    /// 음이 유지되는 중 (Decay → Sustain)
    Decay,
    /// 음이 지속되는 중 (Sustain)
    Sustain,
    /// 음이 꺼지는 중 (Release)
    Release,
}

/// 보이스 (하나의 음)
#[derive(Debug)]
pub struct Voice {
    /// 보이스 상태
    pub state: VoiceState,
    /// 현재 샘플 인덱스 (샘플 내에서의 상대 위치, 0 ~ sample_len, f32 사용으로 CPU 절약)
    position: f32,
    /// 샘플 데이터 (Arc로 공유)
    sample_data: Option<Arc<Vec<i16>>>,
    /// 샘플 시작 오프셋 (샘플 내에서의 시작 위치)
    start_offset: u32,
    /// 샘플 길이
    sample_len: u32,
    /// 루프 시작 인덱스 (샘플 내 상대)
    loop_start: u32,
    /// 루프 종료 인덱스 (샘플 내 상대)
    loop_end: u32,
    /// 루프 모드 (0: 한 번, 1: 루프 Continuous, 2: 루프 until Note-Off, 3: One-Shot)
    loop_mode: u8,
    /// 피치 배율 (1.0 = 원래 음높이)
    pitch_ratio: f32,
    /// 기본 피치 배율
    base_pitch_ratio: f32,
    /// 팬팬 (0.0 = 왼쪽, 0.5 = 중앙, 1.0 = 오른쪽)
    pub pan: f32,
    /// 볼륨
    volume: f32,
    /// ADSR 엔벨로프
    adsr: ADSREnvelope,
    /// 샘플 레이트
    sample_rate: f32,
    /// 필터
    filter: BiquadFilter,
    /// 모듈레이션 LFO
    mod_lfo: LFO,
    /// 비브라토 LFO
    vib_lfo: LFO,
    /// 모듈레이션 깊이
    mod_depth: f32,
    /// 첫 샘플 값 (팝 노이즈 방지용)
    first_sample: f32,
    /// 마지막 출력 값 (release 시 fade-out)
    last_output: f32,
    /// 보이스가 릴리즈된 후 경과 샘플 수 (fade-out용)
    release_counter: u32,
    /// 보이스가 속한 MIDI 채널 (피치벤드/CC 적용 추적용)
    pub channel: u8,
    /// 보이스가 트리거된 노트 번호
    pub note: u8,
}

impl Voice {
    /// 새로운 보이스 생성
    pub fn new() -> Self {
        Self {
            state: VoiceState::Off,
            position: 0.0,
            sample_data: None,
            start_offset: 0,
            sample_len: 0,
            loop_start: 0,
            loop_end: 0,
            loop_mode: 0,
            pitch_ratio: 1.0,
            base_pitch_ratio: 1.0,
            pan: 0.5,
            volume: 1.0,
            adsr: ADSREnvelope::new(),
            sample_rate: 44100.0,
            filter: BiquadFilter::new(44100.0),
            mod_lfo: LFO::new(44100.0),
            vib_lfo: LFO::new(44100.0),
            mod_depth: 0.0,
            first_sample: 0.0,
            last_output: 0.0,
            release_counter: 0,
            channel: 0,
            note: 0,
        }
    }

    /// 음 트리거 (smpl_data의 Arc를 공유)
    pub fn trigger(
        &mut self,
        sample: &Sample,
        smpl_data: Arc<Vec<i16>>,
        note: u8,
        velocity: u8,
        sample_rate: f32,
        attack: f32,
        decay: f32,
        sustain: f32,
        release: f32,
        attenuation: f32,
        channel: u8,
    ) {
        self.channel = channel;
        self.note = note;
        // 첫 샘플 값 미리 계산 (팝 노이즈 방지용)
        let first_sample_val = if smpl_data.is_empty() {
            0.0
        } else {
            smpl_data[sample.start as usize] as f32 / 32768.0
        };

        self.sample_data = Some(smpl_data.clone());

        // Leading silence skip: SF2 spec - sample 시작 부분에 0값이 최소 46개 있을 수 있음
        // 이게 pop noise의 원인. 0이 아닌 값이 처음 나오는 위치를 찾는다.
        let sample_start = sample.start as usize;
        let sample_end = (sample.end as usize).min(smpl_data.len());
        let mut real_start = sample_start;
        for i in sample_start..(sample_start + 200).min(sample_end) {
            if smpl_data[i].abs() > 100 {  // 0이 아닌 의미있는 값
                real_start = i;
                break;
            }
        }
        self.start_offset = real_start as u32;
        self.sample_len = (sample_end as u32).saturating_sub(self.start_offset);

        // sample이 너무 짧으면 (200 sample 미만 = 4.5ms 미만) 진짜 음이 아님. trigger 거부
        if self.sample_len < 200 {
            // sample을 off 상태로 둠 (no sound)
            self.state = VoiceState::Off;
            return;
        }
        // leading silence skip으로 sample이 너무 짧아진 경우도 거부
        if self.sample_len < 1000 {
            self.state = VoiceState::Off;
            return;
        }
        // 루프 포인트는 샘플 내에서의 상대 위치
        self.loop_start = sample.start_loop.saturating_sub(self.start_offset);
        self.loop_end = sample.end_loop.saturating_sub(self.start_offset);
        // 루프: SF2 spec - 53번 generator (SampleModes)로 결정
        // 0=NoLoop, 1=Continuous, 2=UntilRelease, 3=OneShot
        // 우리는 진짜 음을 위해 Continuous만 지원. 짧은 sample은 loop 안 함
        if self.loop_end > self.loop_start
            && self.loop_end <= self.sample_len
            && (self.loop_end - self.loop_start) > 1000  // loop 구간이 1000 샘플 이상
        {
            self.loop_mode = 1; // Continuous loop (긴 sample만)
        } else {
            self.loop_mode = 0; // No loop (짧은 sample은 반복 안 함)
        }
        self.sample_rate = sample_rate;

        // 필터 초기화
        self.filter = BiquadFilter::new(sample_rate);
        self.filter.set_lowpass(20000.0, 0.707, sample_rate);

        // LFO 초기화
        self.mod_lfo = LFO::new(sample_rate);
        self.mod_lfo.set_params(5.0, 0.0, 0.0);
        self.vib_lfo = LFO::new(sample_rate);
        self.vib_lfo.set_params(5.0, 0.0, 0.0);

        // 피치 계산
        let original_pitch = sample.original_pitch as f32;
        let pitch_correction = sample.pitch_correction as f32;

        // MIDI 노트에서 샘플 레이트로의 비율
        let note_diff = note as f32 - original_pitch;
        let cents = note_diff * 100.0 + pitch_correction;
        self.base_pitch_ratio = (2.0_f32).powf(cents / 1200.0);
        self.pitch_ratio = self.base_pitch_ratio;

        // 볼륨 (velocity 기반)
        self.volume = (velocity as f32 / 127.0).clamp(0.0, 1.0);

        // ADSR 초기화 (SF2 envelope generator 값 적용)
        let sustain = sustain.clamp(0.0, 1.0);
        let atten_gain = 10f32.powf(-attenuation / 200.0);
        self.volume *= atten_gain;
        // 첫 샘플이 0이 아니면 attack을 0.05초로 늘려서 pop noise 방지
        let attack = if first_sample_val.abs() > 0.01 {
            attack.max(0.05)
        } else {
            attack.max(0.001)
        };
        self.adsr.set_params(attack, decay.max(0.001), sustain, release.max(0.001));
        self.adsr.trigger();
        self.state = VoiceState::Attack;

        self.position = 0.0;
        self.mod_depth = 0.0;

        // 첫 샘플 값 기록 (팝 노이즈 방지용)
        self.first_sample = first_sample_val;
        self.last_output = 0.0;
        self.release_counter = 0;

        // 필터 리셋 (이전 보이스의 state 제거)
        self.filter.reset();
    }

    /// 볼륨 설정
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    /// 피치벤드 적용
    pub fn apply_pitch_bend(&mut self, bend: f32, sensitivity: f32) {
        // bend: -8192 ~ 8191
        // sensitivity: semitones
        let bend_cents = (bend / 8192.0) * (sensitivity * 100.0);
        let bend_ratio = (2.0_f32).powf(bend_cents / 1200.0);
        self.pitch_ratio = self.base_pitch_ratio * bend_ratio;
    }

    /// 모듈레이션 깊이 설정
    pub fn set_modulation_depth(&mut self, depth: f32) {
        self.mod_depth = depth;
    }

    /// 필터 설정 (CC71:resonance 0-127, CC74:cutoff 0-127)
    pub fn set_filter(&mut self, cutoff: f32, resonance: f32) {
        // 컷오프: 0-127 -> 20Hz-20000Hz (지수 스케일)
        let cutoff_hz = 20.0_f32 * (20000.0_f32 / 20.0_f32).powf(cutoff / 127.0);
        // 리조넌스: 0-127 -> Q 0.1-15
        let q = 0.1 + (resonance / 127.0) * 14.9;
        self.filter.set_lowpass(cutoff_hz, q, self.sample_rate);
    }

    /// 비브라토 깊이 설정 (CC76: 0-127)
    pub fn set_vibrato_depth(&mut self, depth: f32) {
        let vib_depth = depth / 127.0 * 0.05; // 최대 ±5% 피치 변조
        self.vib_lfo.set_depth(vib_depth);
    }

    /// 비브라토 레이트 설정 (Hz)
    pub fn set_vibrato_rate(&mut self, rate: f32) {
        self.vib_lfo.set_freq(rate.clamp(0.1, 20.0));
    }

    /// ADSR 설정 (SF2 제네레이터 값 -> 초 단위로 변환)
    /// attack/decay/release: SF2 timecents (-12000~8000) -> 초
    /// sustain: 0~1000 -> 0.0~1.0
    pub fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.adsr.set_params_from_timecents(attack, decay, sustain, release);
    }

    /// 음 정지 (릴리즈)
    pub fn release(&mut self) {
        if self.state != VoiceState::Off {
            self.adsr.start_release();
            self.state = VoiceState::Release;
            self.release_counter = 0;
        }
    }

    /// 샘플 하나 렌더링
    pub fn render_sample(&mut self) -> (f32, f32) {
        if self.state == VoiceState::Off {
            return (0.0, 0.0);
        }

        // ADSR 업데이트
        self.adsr.update(self.sample_rate);

        // 현재 상태 확인
        self.state = match self.adsr.state {
            ADSRState::Attack => VoiceState::Attack,
            ADSRState::Decay => VoiceState::Decay,
            ADSRState::Sustain => VoiceState::Sustain,
            ADSRState::Release => VoiceState::Release,
            ADSRState::Off => VoiceState::Off,
        };

        // ADSR가 끝났으면 음 off
        if self.adsr.is_finished() {
            self.state = VoiceState::Off;
            return (0.0, 0.0);
        }

        // 샘플 데이터가 없으면 0 반환
        let sample_data = match &self.sample_data {
            Some(d) => d,
            None => return (0.0, 0.0),
        };

        // 모듈레이션 LFO (피치 변조)
        let mod_lfo_val = self.mod_lfo.sine() * self.mod_depth;

        // 비브라토 LFO
        let vib_lfo_val = self.vib_lfo.sine() * 0.02; // 기본적으로 작은 값

        // 피치 변조 적용
        let modulated_pitch = self.pitch_ratio * (1.0 + mod_lfo_val * 0.1 + vib_lfo_val);

        // 샘플 가져오기 (Linear interpolation: 빠름, cubic은 CPU 많이 먹음)
        let sample_val = self.interpolate_sample_with_pitch(sample_data, modulated_pitch as f32);

        // 필터 적용 (LPF)
        let filtered = self.filter.process(sample_val);

        // ADSR 레벨 적용
        let level = self.adsr.get_level();
        let mut output = filtered * level * self.volume;

        // 팝 노이즈 방지: release 시 fade-out
        if self.state == VoiceState::Release {
            self.release_counter += 1;
            let fade_samples = 256; // 약 5.8ms @ 44.1kHz
            if self.release_counter < fade_samples {
                let fade_gain = 1.0 - (self.release_counter as f32 / fade_samples as f32);
                output *= fade_gain;
            }
        }

        self.last_output = output;

        // 위치 업데이트 (f32 사용으로 CPU 절약)
        self.position += self.pitch_ratio;

        // 루프 또는 끝 처리
        if (self.position as u32) >= self.sample_len {
            // 샘플 끝
            match self.loop_mode {
                0 | 1 => {
                    // 루프
                    if self.loop_end > self.loop_start {
                        let loop_len = self.loop_end - self.loop_start;
                        let rel_pos = (self.position as u32 - self.loop_start) % loop_len;
                        self.position = self.loop_start as f32 + rel_pos as f32;
                    } else {
                        self.state = VoiceState::Off;
                    }
                }
                2 => {
                    // 루프 until Note-Off (릴리즈 시 루프에서 나옴)
                    if self.state != VoiceState::Release {
                        if self.loop_end > self.loop_start {
                            let loop_len = self.loop_end - self.loop_start;
                            let rel_pos = (self.position as u32 - self.loop_start) % loop_len;
                            self.position = self.loop_start as f32 + rel_pos as f32;
                        }
                    } else {
                        self.state = VoiceState::Off;
                    }
                }
                3 => {
                    // One-Shot
                    self.state = VoiceState::Off;
                }
                _ => {
                    self.state = VoiceState::Off;
                }
            }
        }

        // 패닝 적용 (모노 샘플 -> 스테레오)
        let left = output * (1.0 - self.pan);
        let right = output * self.pan;

        (left, right)
    }

    /// 선형 보간으로 샘플 가져오기
    fn interpolate_sample(&self) -> f32 {
        match &self.sample_data {
            Some(d) => self.interpolate_sample_with_pitch(d, self.position as f32),
            None => 0.0,
        }
    }

    /// 특정 피치로 샘플 보간 (샘플 내 상대 위치, smpl_data 전체 참조)
    fn interpolate_sample_with_pitch(&self, sample_data: &Arc<Vec<i16>>, rel_pos: f32) -> f32 {
        // 절대 인덱스 계산 (f32 사용)
        let abs_idx = self.start_offset as f32 + rel_pos;
        let idx = abs_idx as usize;
        let frac = abs_idx - idx as f32;

        if idx >= sample_data.len().saturating_sub(1) {
            return sample_data.last().copied().unwrap_or(0) as f32 / 32768.0;
        }

        let s1 = sample_data[idx] as f32;
        let s2 = sample_data[idx + 1] as f32;

        // 선형 보간
        let sample = s1 + (s2 - s1) * frac;
        sample / 32768.0
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

/// 보이스 관리자
pub struct VoiceManager {
    /// 보이스 풀
    pub voices: Vec<Voice>,
    /// 최대 보이스 수
    max_voices: usize,
}

impl VoiceManager {
    /// 새로운 보이스 관리자 생성
    pub fn new(max_voices: usize) -> Self {
        Self {
            voices: Vec::with_capacity(max_voices),
            max_voices,
        }
    }

    /// 모든 보이스 초기화
    pub fn reset(&mut self) {
        self.voices.clear();
    }

    /// 사용 가능한 보이스 찾기
    pub fn find_free_voice(&mut self) -> Option<&mut Voice> {
        // 먼저 꺼진 보이스 찾기
        if let Some(idx) = self.voices.iter().position(|v| v.state == VoiceState::Off) {
            return Some(&mut self.voices[idx]);
        }

        // 없으면 새 보이스 추가 (최대 개수 이하)
        if self.voices.len() < self.max_voices {
            self.voices.push(Voice::new());
            return self.voices.last_mut();
        }

        // 가장 오래된 보이스 찾기 (단순 FIFO)
        if let Some(idx) = self.voices.iter().position(|v| v.state == VoiceState::Sustain || v.state == VoiceState::Decay) {
            self.voices[idx].release();
            return Some(&mut self.voices[idx]);
        }

        None
    }

    /// 활성 보이스 수
    pub fn active_count(&self) -> usize {
        self.voices.iter().filter(|v| v.state != VoiceState::Off).count()
    }
}

impl Default for VoiceManager {
    fn default() -> Self {
        Self::new(256)
    }
}

/// ADSR 엔벨로프 상태
#[derive(Debug, Clone, Copy, PartialEq)]
enum ADSRState {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Default for ADSRState {
    fn default() -> Self {
        ADSRState::Off
    }
}

/// ADSR 엔벨로프
#[derive(Debug, Clone, Copy)]
struct ADSREnvelope {
    /// 현재 상태
    state: ADSRState,
    /// 현재 레벨
    level: f32,
    /// 어택 속도 (레벨/초)
    attack_rate: f32,
    /// 디케이 속도 (레벨/초)
    decay_rate: f32,
    /// 서스테인 레벨
    sustain_level: f32,
    /// 릴리즈 속도 (레벨/초)
    release_rate: f32,
    /// 릴리즈 시작 레벨
    release_start_level: f32,
}

impl ADSREnvelope {
    /// 새로운 ADSR 생성
    fn new() -> Self {
        Self {
            state: ADSRState::Off,
            level: 0.0,
            attack_rate: 0.0,
            decay_rate: 0.0,
            sustain_level: 1.0,
            release_rate: 0.0,
            release_start_level: 0.0,
        }
    }

    /// ADSR 파라미터 설정 (시간 -> 속도로 변환)
    fn set_params(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        // 속도 = 1.0 / 시간 (초为单位)
        self.attack_rate = if attack > 0.001 { 1.0 / attack } else { 1000.0 };
        self.decay_rate = if decay > 0.001 { 1.0 / decay } else { 1000.0 };
        self.sustain_level = sustain.clamp(0.0, 1.0);
        self.release_rate = if release > 0.001 { 1.0 / release } else { 1000.0 };
    }

    /// SF2 timecents 단위에서 ADSR 파라미터 설정
    /// timecents: 2^(tc/1200) 초
    /// sustain: 0~1000 (SF2 단위) -> 0.0~1.0
    pub fn set_params_from_timecents(&mut self, attack_tc: f32, decay_tc: f32, sustain: f32, release_tc: f32) {
        let attack = timecents_to_seconds(attack_tc);
        let decay = timecents_to_seconds(decay_tc);
        let release = timecents_to_seconds(release_tc);
        let sustain_norm = sustain / 1000.0;
        self.set_params(attack, decay, sustain_norm, release);
    }

    /// 트리거 (어택 시작)
    fn trigger(&mut self) {
        self.state = ADSRState::Attack;
    }

    /// 릴리즈 시작
    fn start_release(&mut self) {
        if self.state != ADSRState::Off {
            self.state = ADSRState::Release;
            self.release_start_level = self.level;
        }
    }

    /// 업데이트 (1 프레임마다 호출)
    fn update(&mut self, sample_rate: f32) {
        // 샘플 단위时间来除以采样率得到秒
        let step = sample_rate.recip(); // 1/sample_rate

        match self.state {
            ADSRState::Off => {
                self.level = 0.0;
            }
            ADSRState::Attack => {
                self.level += self.attack_rate * step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.state = ADSRState::Decay;
                }
            }
            ADSRState::Decay => {
                self.level -= self.decay_rate * step;
                if self.level <= self.sustain_level {
                    self.level = self.sustain_level;
                    self.state = ADSRState::Sustain;
                }
            }
            ADSRState::Sustain => {
                // 서스테인 상태에서는 레벨이 변하지 않음
                self.level = self.sustain_level;
            }
            ADSRState::Release => {
                self.level -= self.release_rate * step * self.release_start_level;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.state = ADSRState::Off;
                }
            }
        }
    }

    /// 현재 레벨 가져오기
    fn get_level(&self) -> f32 {
        self.level
    }

    /// 끝났는지 확인
    fn is_finished(&self) -> bool {
        self.state == ADSRState::Off
    }
}

impl Default for ADSREnvelope {
    fn default() -> Self {
        Self::new()
    }
}

/// SF2 timecents를 초 단위로 변환
/// timecents: 2^(tc/1200) 초
/// -12000 timecents는 1ms (최소값)
pub fn timecents_to_seconds(tc: f32) -> f32 {
    if tc <= -32768.0 {
        return 0.001; // 1ms
    }
    (2.0_f32).powf(tc / 1200.0).max(0.001)
}

/// 보이스의 ADSR 상태 (외부 조회용)
impl Voice {
    /// ADSR 공격 시간 (초)
    pub fn attack_time(&self) -> f32 {
        1.0 / self.adsr.attack_rate
    }

    /// ADSR 릴리즈 시간 (초)
    pub fn release_time(&self) -> f32 {
        1.0 / self.adsr.release_rate
    }
}

/// Cubic 보간으로 샘플 가져오기 (고품질 - 현재 미사용이지만 API로 노출)
impl Voice {
    /// Cubic 보간 샘플 읽기
    pub fn interpolate_cubic(&self, pos: f32) -> f32 {
        let sample_data = match &self.sample_data {
            Some(d) => d,
            None => return 0.0,
        };
        let abs_idx = self.start_offset as f32 + pos;
        let idx = abs_idx as i64;
        let frac = abs_idx - idx as f32;

        if idx < 1 || (idx + 2) as usize >= sample_data.len() {
            return self.interpolate_sample_with_pitch(sample_data, pos);
        }

        let s0 = sample_data[(idx - 1) as usize] as f32;
        let s1 = sample_data[idx as usize] as f32;
        let s2 = sample_data[(idx + 1) as usize] as f32;
        let s3 = sample_data[(idx + 2) as usize] as f32;

        // Catmull-Rom cubic interpolation
        let a0 = -0.5 * s0 + 1.5 * s1 - 1.5 * s2 + 0.5 * s3;
        let a1 = s0 - 2.5 * s1 + 2.0 * s2 - 0.5 * s3;
        let a2 = -0.5 * s0 + 0.5 * s2;
        let a3 = s1;

        let result = a0 * frac * frac * frac + a1 * frac * frac + a2 * frac + a3;
        result / 32768.0
    }
}
