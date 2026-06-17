// dsp.rs - DSP 모듈 (필터, LFO, 이펙트)

/// 2차 Biquad 필터
#[derive(Debug, Clone, Copy)]
pub struct BiquadFilter {
    /// 필터 a1
    a1: f32,
    /// 필터 a2
    a2: f32,
    /// 필터 b0
    b0: f32,
    /// 필터 b1
    b1: f32,
    /// 필터 b2
    b2: f32,
    /// 지연 버퍼
    z1: f32,
    z2: f32,
    /// 컷오프 주파수
    cutoff: f32,
    /// Q 값
    q: f32,
    /// 필터 타입
    filter_type: FilterType,
}

/// 필터 타입
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

impl BiquadFilter {
    /// 새로운 필터 생성
    pub fn new(sample_rate: f32) -> Self {
        Self {
            a1: 0.0,
            a2: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            z1: 0.0,
            z2: 0.0,
            cutoff: 20000.0,
            q: 0.707,
            filter_type: FilterType::LowPass,
        }
    }

    /// 필터 설정 (공진 포함)
    pub fn set_params(&mut self, filter_type: FilterType, cutoff: f32, q: f32, gain: f32, sample_rate: f32) {
        let omega = 2.0 * std::f32::consts::PI * cutoff / sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);
        let a = 10.0_f32.powf(gain / 40.0);

        match filter_type {
            FilterType::LowPass => {
                self.b0 = (1.0 - cos_omega) / 2.0;
                self.b1 = 1.0 - cos_omega;
                self.b2 = (1.0 - cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_omega / a0;
                self.a2 = (1.0 - alpha) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
            FilterType::HighPass => {
                self.b0 = (1.0 + cos_omega) / 2.0;
                self.b1 = -(1.0 + cos_omega);
                self.b2 = (1.0 + cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_omega / a0;
                self.a2 = (1.0 - alpha) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
            FilterType::BandPass => {
                self.b0 = alpha;
                self.b1 = 0.0;
                self.b2 = -alpha;
                let a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_omega / a0;
                self.a2 = (1.0 - alpha) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
            FilterType::Notch => {
                self.b0 = 1.0;
                self.b1 = -2.0 * cos_omega;
                self.b2 = 1.0;
                let a0 = 1.0 + alpha;
                self.a1 = -2.0 * cos_omega / a0;
                self.a2 = (1.0 - alpha) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
            FilterType::Peak => {
                self.b0 = 1.0 + alpha * a;
                self.b1 = -2.0 * cos_omega;
                self.b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                self.a1 = -2.0 * cos_omega / a0;
                self.a2 = (1.0 - alpha / a) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
            FilterType::LowShelf => {
                let sqrt_a = a.sqrt();
                self.b0 = a * ((a + 1.0) - (a - 1.0) * cos_omega + 2.0 * sqrt_a * alpha);
                self.b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_omega);
                self.b2 = a * ((a + 1.0) - (a - 1.0) * cos_omega - 2.0 * sqrt_a * alpha);
                let a0 = (a + 1.0) + (a - 1.0) * cos_omega + 2.0 * sqrt_a * alpha;
                self.a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_omega) / a0;
                self.a2 = ((a + 1.0) + (a - 1.0) * cos_omega - 2.0 * sqrt_a * alpha) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
            FilterType::HighShelf => {
                let sqrt_a = a.sqrt();
                self.b0 = a * ((a + 1.0) + (a - 1.0) * cos_omega + 2.0 * sqrt_a * alpha);
                self.b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_omega);
                self.b2 = a * ((a + 1.0) + (a - 1.0) * cos_omega - 2.0 * sqrt_a * alpha);
                let a0 = (a + 1.0) - (a - 1.0) * cos_omega + 2.0 * sqrt_a * alpha;
                self.a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_omega) / a0;
                self.a2 = ((a + 1.0) - (a - 1.0) * cos_omega - 2.0 * sqrt_a * alpha) / a0;
                self.b0 /= a0;
                self.b1 /= a0;
                self.b2 /= a0;
            }
        }

        self.cutoff = cutoff;
        self.q = q;
        self.filter_type = filter_type;
    }

    /// LowPass 필터 설정
    pub fn set_lowpass(&mut self, cutoff: f32, q: f32, sample_rate: f32) {
        self.set_params(FilterType::LowPass, cutoff, q, 0.0, sample_rate);
    }

    /// HighPass 필터 설정
    pub fn set_highpass(&mut self, cutoff: f32, q: f32, sample_rate: f32) {
        self.set_params(FilterType::HighPass, cutoff, q, 0.0, sample_rate);
    }

    /// BandPass 필터 설정
    pub fn set_bandpass(&mut self, cutoff: f32, q: f32, sample_rate: f32) {
        self.set_params(FilterType::BandPass, cutoff, q, 0.0, sample_rate);
    }

    /// 필터 처리
    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }

    /// 필터 초기화
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// 컷오프 주파수 설정 (0.0-1.0, normalized)
    pub fn set_cutoff_normalized(&mut self, norm: f32, sample_rate: f32) {
        let cutoff = 20.0 + norm * (sample_rate / 2.0 - 20.0);
        let q = self.q;
        self.set_params(self.filter_type, cutoff, q, 0.0, sample_rate);
    }
}

impl Default for BiquadFilter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// LFO (저주파 오실레이터)
#[derive(Debug, Clone, Copy)]
pub struct LFO {
    /// 위상 (0.0 - 1.0)
    phase: f32,
    /// 주파수 (Hz)
    freq: f32,
    /// 지연 시간 (초)
    delay: f32,
    /// 현재 지연 경과 시간
    delay_time: f32,
    /// 진폭 (0.0 - 1.0)
    depth: f32,
    /// 샘플 레이트
    sample_rate: f32,
}

impl LFO {
    /// 새로운 LFO 생성
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            freq: 5.0,
            delay: 0.0,
            delay_time: 0.0,
            depth: 1.0,
            sample_rate,
        }
    }

    /// 파라미터 설정
    pub fn set_params(&mut self, freq: f32, delay: f32, depth: f32) {
        self.freq = freq.max(0.001);
        self.delay = delay.max(0.0);
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// 주파수 설정
    pub fn set_freq(&mut self, freq: f32) {
        self.freq = freq.max(0.001);
    }

    /// 진폭 설정
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// 위상 설정
    pub fn set_phase(&mut self, phase: f32) {
        self.phase = phase % 1.0;
        if self.phase < 0.0 {
            self.phase += 1.0;
        }
    }

    /// 사인파 출력
    pub fn sine(&mut self) -> f32 {
        self.update_delay();
        if self.delay_time < self.delay {
            return 0.0;
        }
        let t = (self.delay_time - self.delay) / (1.0 / self.freq).max(0.001);
        let effective_phase = (t * self.freq) % 1.0;
        let wave = (effective_phase * 2.0 * std::f32::consts::PI).sin();
        wave * self.depth
    }

    /// 삼각파 출력
    pub fn triangle(&mut self) -> f32 {
        self.update_delay();
        if self.delay_time < self.delay {
            return 0.0;
        }
        let t = (self.delay_time - self.delay) * self.freq;
        let phase = t % 1.0;
        let wave = if phase < 0.5 {
            4.0 * phase - 1.0
        } else {
            3.0 - 4.0 * phase
        };
        wave * self.depth
    }

    /// 사각파 출력
    pub fn square(&mut self) -> f32 {
        self.update_delay();
        if self.delay_time < self.delay {
            return 0.0;
        }
        let t = (self.delay_time - self.delay) * self.freq;
        let phase = t % 1.0;
        let wave = if phase < 0.5 { 1.0 } else { -1.0 };
        wave * self.depth
    }

    /// 톱니파 출력
    pub fn sawtooth(&mut self) -> f32 {
        self.update_delay();
        if self.delay_time < self.delay {
            return 0.0;
        }
        let t = (self.delay_time - self.delay) * self.freq;
        let phase = t % 1.0;
        let wave = 2.0 * phase - 1.0;
        wave * self.depth
    }

    /// 지연 업데이트
    fn update_delay(&mut self) {
        self.delay_time += 1.0 / self.sample_rate;
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.delay_time = 0.0;
    }
}

impl Default for LFO {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// 코러스 이펙트
#[derive(Debug, Clone)]
pub struct Chorus {
    /// 지연 버퍼 (최대 100ms)
    delay_buffer: Vec<f32>,
    /// 지연 버퍼 쓰기 위치
    write_pos: usize,
    /// 지연 시간 (샘플)
    delay_samples: f32,
    /// 변조 깊이 (샘플)
    mod_depth: f32,
    /// 변조 LFO
    lfo: LFO,
    /// 모드 (0: 모노 in/스테레오 out, 1: 스테레오)
    mode: u8,
    /// 필터
    filter: BiquadFilter,
    /// 샘플 레이트
    sample_rate: f32,
}

impl Chorus {
    /// 새로운 코러스 생성
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 0.1) as usize; // 100ms
        Self {
            delay_buffer: vec![0.0; max_delay],
            write_pos: 0,
            delay_samples: sample_rate * 0.025, // 25ms 기본
            mod_depth: sample_rate * 0.002, // 2ms 변조
            lfo: LFO::new(sample_rate),
            mode: 0,
            filter: BiquadFilter::new(sample_rate),
            sample_rate,
        }
    }

    /// 파라미터 설정
    pub fn set_params(&mut self, rate: f32, depth: f32, delay_ms: f32) {
        self.lfo.set_freq(rate);
        self.mod_depth = depth * self.sample_rate * 0.005; // 0-5ms
        self.delay_samples = delay_ms * self.sample_rate / 1000.0;
        self.delay_samples = self.delay_samples.clamp(1.0, self.delay_buffer.len() as f32 - 1.0);
    }

    /// 모드 설정 (0: 모노, 1: 스테레오)
    pub fn set_mode(&mut self, mode: u8) {
        self.mode = mode;
    }

    /// 처리 (모노 입력)
    pub fn process(&mut self, input: f32) -> f32 {
        // LFO로 지연 시간 변조
        let mod_lfo = self.lfo.sine();
        let current_delay = (self.delay_samples + mod_lfo * self.mod_depth).clamp(1.0, self.delay_buffer.len() as f32 - 1.0);

        // 지연된 샘플 읽기 (선형 보간)
        let read_pos = if self.write_pos as f32 >= current_delay {
            self.write_pos as f32 - current_delay
        } else {
            self.write_pos as f32 - current_delay + self.delay_buffer.len() as f32
        };

        let int_pos = read_pos as usize;
        let frac = read_pos - int_pos as f32;
        let next_pos = (int_pos + 1) % self.delay_buffer.len();

        let delayed = self.delay_buffer[int_pos] * (1.0 - frac) + self.delay_buffer[next_pos] * frac;

        // 원본 + 지연 신호 혼합
        let output = input + delayed * 0.5;

        // 현재 입력 버퍼에 저장
        self.delay_buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % self.delay_buffer.len();

        output
    }

    /// 처리 (스테레오 입력)
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        // 왼쪽: 원본 + 오른쪽 지연
        let mod_lfo = self.lfo.sine();
        let current_delay = (self.delay_samples + mod_lfo * self.mod_depth).clamp(1.0, self.delay_buffer.len() as f32 - 1.0);

        let read_pos = if self.write_pos as f32 >= current_delay {
            self.write_pos as f32 - current_delay
        } else {
            self.write_pos as f32 - current_delay + self.delay_buffer.len() as f32
        };

        let int_pos = read_pos as usize;
        let frac = read_pos - int_pos as f32;
        let next_pos = (int_pos + 1) % self.delay_buffer.len();

        let delayed = self.delay_buffer[int_pos] * (1.0 - frac) + self.delay_buffer[next_pos] * frac;

        // 왼쪽: 원본 + 지연
        let out_l = left + delayed * 0.5;
        // 오른쪽: 지연 + 원본 (위상 차이)
        let out_r = right + delayed * 0.5 * (1.0 - mod_lfo.abs() * 0.3);

        // 현재 입력 버퍼에 저장
        self.delay_buffer[self.write_pos] = (left + right) * 0.5;
        self.write_pos = (self.write_pos + 1) % self.delay_buffer.len();

        (out_l, out_r)
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.delay_buffer.fill(0.0);
        self.write_pos = 0;
        self.lfo.reset();
    }
}

impl Default for Chorus {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// 리버브 이펙트 (단순한 Schroeder 리버브)
#[derive(Debug, Clone)]
pub struct Reverb {
    /// 콤브 필터 1
    comb1: CombFilter,
    /// 콤브 필터 2
    comb2: CombFilter,
    /// 콤브 필터 3
    comb3: CombFilter,
    /// 콤브 필터 4
    comb4: CombFilter,
    /// 올패스 필터 1
    allpass1: AllPassFilter,
    /// 올패스 필터 2
    allpass2: AllPassFilter,
    /// 스테레오 딜레이
    stereo_delay_l: Vec<f32>,
    stereo_delay_r: Vec<f32>,
    stereo_pos_l: usize,
    stereo_pos_r: usize,
    /// 혼합 비율
    wet: f32,
    /// 샘플 레이트
    sample_rate: f32,
}

/// 콤브 필터
#[derive(Debug, Clone)]
struct CombFilter {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
    damping: f32,
    filterstore: f32,
}

impl CombFilter {
    fn new(delay_ms: f32, feedback: f32, damping: f32, sample_rate: f32) -> Self {
        let size = (delay_ms * sample_rate / 1000.0) as usize;
        Self {
            buffer: vec![0.0; size],
            pos: 0,
            feedback: feedback.clamp(0.0, 0.98),
            damping: damping.clamp(0.0, 0.4),
            filterstore: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.pos];
        self.filterstore = output * (1.0 - self.damping) + self.filterstore * self.damping;
        self.buffer[self.pos] = input + self.filterstore * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filterstore = 0.0;
    }
}

/// 올패스 필터
#[derive(Debug, Clone)]
struct AllPassFilter {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl AllPassFilter {
    fn new(delay_ms: f32, feedback: f32, sample_rate: f32) -> Self {
        let size = (delay_ms * sample_rate / 1000.0) as usize;
        Self {
            buffer: vec![0.0; size],
            pos: 0,
            feedback: feedback.clamp(0.0, 0.98),
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.pos];
        let output = -input + delayed;
        self.buffer[self.pos] = input + delayed * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
    }
}

impl Reverb {
    /// 새로운 리버브 생성
    pub fn new(sample_rate: f32) -> Self {
        Self {
            comb1: CombFilter::new(29.7, 0.805, 0.22, sample_rate),
            comb2: CombFilter::new(37.1, 0.827, 0.20, sample_rate),
            comb3: CombFilter::new(41.1, 0.841, 0.18, sample_rate),
            comb4: CombFilter::new(43.7, 0.850, 0.16, sample_rate),
            allpass1: AllPassFilter::new(5.0, 0.7, sample_rate),
            allpass2: AllPassFilter::new(1.7, 0.7, sample_rate),
            stereo_delay_l: vec![0.0; (sample_rate * 0.05) as usize], // 50ms
            stereo_delay_r: vec![0.0; (sample_rate * 0.07) as usize], // 70ms
            stereo_pos_l: 0,
            stereo_pos_r: 0,
            wet: 0.3,
            sample_rate,
        }
    }

    /// 파라미터 설정 (0.0 - 1.0)
    pub fn set_params(&mut self, size: f32, damping: f32) {
        // 사이즈에 따른 피드백 조절
        let base_feedback = 0.7 + size * 0.2;
        self.comb1.feedback = base_feedback + 0.1;
        self.comb2.feedback = base_feedback + 0.05;
        self.comb3.feedback = base_feedback;
        self.comb4.feedback = base_feedback - 0.05;

        self.wet = 0.2 + size * 0.4;
    }

    /// 처리
    pub fn process(&mut self, input: f32) -> f32 {
        // 병렬 콤브 필터
        let mut out = 0.0;
        out += self.comb1.process(input);
        out += self.comb2.process(input);
        out += self.comb3.process(input);
        out += self.comb4.process(input);

        // 직렬 올패스 필터
        out = self.allpass1.process(out);
        out = self.allpass2.process(out);

        out * self.wet
    }

    /// 처리 (스테레오)
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        // 기본 리버브
        let mono = (left + right) * 0.5;
        let wet_mono = self.process(mono);

        // 스테레오 딜레이
        let delay_l = self.stereo_delay_l[self.stereo_pos_l];
        let delay_r = self.stereo_delay_r[self.stereo_pos_r];

        self.stereo_delay_l[self.stereo_pos_l] = left + wet_mono * 0.3;
        self.stereo_delay_r[self.stereo_pos_r] = right + wet_mono * 0.3;

        self.stereo_pos_l = (self.stereo_pos_l + 1) % self.stereo_delay_l.len();
        self.stereo_pos_r = (self.stereo_pos_r + 1) % self.stereo_delay_r.len();

        (left + delay_r * 0.2, right + delay_l * 0.2)
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.comb1.reset();
        self.comb2.reset();
        self.comb3.reset();
        self.comb4.reset();
        self.allpass1.reset();
        self.stereo_delay_l.fill(0.0);
        self.stereo_delay_r.fill(0.0);
        self.stereo_pos_l = 0;
        self.stereo_pos_r = 0;
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// 딜레이 이펙트
#[derive(Debug, Clone)]
pub struct Delay {
    /// 딜레이 버퍼 L
    buffer_l: Vec<f32>,
    /// 딜레이 버퍼 R
    buffer_r: Vec<f32>,
    /// 쓰기 위치
    write_pos: usize,
    /// 딜레이 시간 (샘플)
    delay_l: usize,
    /// 딜레이 시간 R (샘플)
    delay_r: usize,
    /// 피드백
    feedback: f32,
    /// 모드 (0: 모노, 1: 핑퐁, 2: 스테레오)
    mode: u8,
}

impl Delay {
    /// 새로운 딜레이 생성
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 2.0) as usize; // 2초
        Self {
            buffer_l: vec![0.0; max_delay],
            buffer_r: vec![0.0; max_delay],
            write_pos: 0,
            delay_l: (sample_rate * 0.25) as usize, // 250ms
            delay_r: (sample_rate * 0.375) as usize, // 375ms
            feedback: 0.3,
            mode: 0,
        }
    }

    /// 시간 설정 (ms)
    pub fn set_time(&mut self, time_l: f32, time_r: f32, sample_rate: f32) {
        self.delay_l = ((time_l * sample_rate / 1000.0) as usize).min(self.buffer_l.len() - 1);
        self.delay_r = ((time_r * sample_rate / 1000.0) as usize).min(self.buffer_r.len() - 1);
    }

    /// 피드백 설정
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.95);
    }

    /// 모드 설정
    pub fn set_mode(&mut self, mode: u8) {
        self.mode = mode;
    }

    /// 처리 (모노)
    pub fn process(&mut self, input: f32) -> f32 {
        let out_l = self.buffer_l[self.write_pos];
        self.buffer_l[self.write_pos] = input + out_l * self.feedback;
        out_l
    }

    /// 처리 (스테레오)
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        match self.mode {
            0 => {
                // 모노 딜레이
                let out_l = self.buffer_l[self.write_pos];
                self.buffer_l[self.write_pos] = left + out_l * self.feedback;
                (out_l, out_l)
            }
            1 => {
                // 핑퐁 딜레이
                let out_l = self.buffer_l[self.write_pos];
                let out_r = self.buffer_r[self.write_pos];
                self.buffer_l[self.write_pos] = left + out_r * self.feedback;
                self.buffer_r[self.write_pos] = right + out_l * self.feedback;
                (out_l, out_r)
            }
            _ => {
                // 스테레오 딜레이
                let read_l = if self.write_pos >= self.delay_l {
                    self.write_pos - self.delay_l
                } else {
                    self.write_pos - self.delay_l + self.buffer_l.len()
                };
                let read_r = if self.write_pos >= self.delay_r {
                    self.write_pos - self.delay_r
                } else {
                    self.write_pos - self.delay_r + self.buffer_r.len()
                };

                let out_l = self.buffer_l[read_l];
                let out_r = self.buffer_r[read_r];

                self.buffer_l[self.write_pos] = left + out_l * self.feedback;
                self.buffer_r[self.write_pos] = right + out_r * self.feedback;

                (out_l, out_r)
            }
        }
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.buffer_l.fill(0.0);
        self.buffer_r.fill(0.0);
        self.write_pos = 0;
    }
}

impl Default for Delay {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// 톤 조절 (피드포워드 필터)
#[derive(Debug, Clone, Copy)]
pub struct ToneControl {
    /// 저역_gain (dB)
    low_gain: f32,
    /// 중역_gain (dB)
    mid_gain: f32,
    /// 고역_gain (dB)
    high_gain: f32,
    /// 저역 컷오프
    low_cutoff: f32,
    /// 고역 컷오프
    high_cutoff: f32,
    /// 필터들
    lowshelf: BiquadFilter,
    highshelf: BiquadFilter,
    peak: BiquadFilter,
}

impl ToneControl {
    /// 새로운 톤 컨트롤 생성
    pub fn new(sample_rate: f32) -> Self {
        let mut tc = Self {
            low_gain: 0.0,
            mid_gain: 0.0,
            high_gain: 0.0,
            low_cutoff: 250.0,
            high_cutoff: 4000.0,
            lowshelf: BiquadFilter::new(sample_rate),
            highshelf: BiquadFilter::new(sample_rate),
            peak: BiquadFilter::new(sample_rate),
        };
        tc.update_filters(sample_rate);
        tc
    }

    /// 파라미터 설정
    pub fn set_params(&mut self, low: f32, mid: f32, high: f32, sample_rate: f32) {
        self.low_gain = low.clamp(-12.0, 12.0);
        self.mid_gain = mid.clamp(-12.0, 12.0);
        self.high_gain = high.clamp(-12.0, 12.0);
        self.update_filters(sample_rate);
    }

    /// 컷오프 설정
    pub fn set_cutoffs(&mut self, low: f32, high: f32, sample_rate: f32) {
        self.low_cutoff = low.clamp(50.0, 500.0);
        self.high_cutoff = high.clamp(1000.0, 10000.0);
        self.update_filters(sample_rate);
    }

    fn update_filters(&mut self, sample_rate: f32) {
        self.lowshelf.set_params(FilterType::LowShelf, self.low_cutoff, 0.707, self.low_gain, sample_rate);
        self.highshelf.set_params(FilterType::HighShelf, self.high_cutoff, 0.707, self.high_gain, sample_rate);
        self.peak.set_params(FilterType::Peak, (self.low_cutoff + self.high_cutoff) / 2.0, 1.0, self.mid_gain, sample_rate);
    }

    /// 처리
    pub fn process(&mut self, input: f32) -> f32 {
        let out = self.lowshelf.process(input);
        let out = self.peak.process(out);
        self.highshelf.process(out)
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.lowshelf.reset();
        self.highshelf.reset();
        self.peak.reset();
    }
}

impl Default for ToneControl {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// 트레몰로 이펙트 (진폭 변조)
#[derive(Debug, Clone)]
pub struct Tremolo {
    /// LFO
    lfo: LFO,
    /// 깊이 (0.0 - 1.0)
    depth: f32,
    /// 샘플 레이트
    sample_rate: f32,
}

impl Tremolo {
    /// 새로운 트레몰로 생성
    pub fn new(sample_rate: f32) -> Self {
        let mut lfo = LFO::new(sample_rate);
        lfo.set_params(5.0, 0.0, 1.0);
        Self {
            lfo,
            depth: 0.5,
            sample_rate,
        }
    }

    /// 파라미터 설정 (rate: Hz, depth: 0.0-1.0)
    pub fn set_params(&mut self, rate: f32, depth: f32) {
        self.lfo.set_params(rate, 0.0, 1.0);
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// 처리
    pub fn process(&mut self, input: f32) -> f32 {
        let lfo_val = self.lfo.sine();
        // 0.0 - 1.0 범위로 변환
        let modulation = 1.0 - self.depth + lfo_val.abs() * self.depth;
        input * modulation
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.lfo.reset();
    }
}

impl Default for Tremolo {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// 페이저 이펙트 (올패스 필터 체인)
#[derive(Debug, Clone)]
pub struct Phaser {
    /// 올패스 필터 1
    ap1: AllPassFilterPhaser,
    /// 올패스 필터 2
    ap2: AllPassFilterPhaser,
    /// 올패스 필터 3
    ap3: AllPassFilterPhaser,
    /// 올패스 필터 4
    ap4: AllPassFilterPhaser,
    /// LFO
    lfo: LFO,
    /// 피드백
    feedback: f32,
    /// 깊이
    depth: f32,
    /// 샘플 레이트
    sample_rate: f32,
}

/// 올패스 필터 (페이저용, modifiable delay)
#[derive(Debug, Clone)]
struct AllPassFilterPhaser {
    buffer: Vec<f32>,
    pos: usize,
    delay: f32,
    coefficient: f32,
}

impl AllPassFilterPhaser {
    fn new(max_delay: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay],
            pos: 0,
            delay: 1.0,
            coefficient: 0.5,
        }
    }

    fn set_params(&mut self, delay: f32, coefficient: f32) {
        self.delay = delay.clamp(1.0, self.buffer.len() as f32 - 1.0);
        self.coefficient = coefficient.clamp(-0.99, 0.99);
    }

    fn process(&mut self, input: f32) -> f32 {
        let read_pos = if self.pos as f32 >= self.delay {
            self.pos as f32 - self.delay
        } else {
            self.pos as f32 - self.delay + self.buffer.len() as f32
        };
        let int_pos = read_pos as usize;
        let frac = read_pos - int_pos as f32;
        let next_pos = (int_pos + 1) % self.buffer.len();

        let delayed = self.buffer[int_pos] * (1.0 - frac) + self.buffer[next_pos] * frac;
        let output = -self.coefficient * input + delayed;
        self.buffer[self.pos] = input + self.coefficient * output;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
    }
}

impl Phaser {
    /// 새로운 페이저 생성
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 0.01) as usize; // 10ms max
        let mut lfo = LFO::new(sample_rate);
        lfo.set_params(0.5, 0.0, 1.0);
        Self {
            ap1: AllPassFilterPhaser::new(max_delay),
            ap2: AllPassFilterPhaser::new(max_delay),
            ap3: AllPassFilterPhaser::new(max_delay),
            ap4: AllPassFilterPhaser::new(max_delay),
            lfo,
            feedback: 0.3,
            depth: 0.5,
            sample_rate,
        }
    }

    /// 파라미터 설정 (rate: Hz, depth: 0.0-1.0, feedback: 0.0-0.95)
    pub fn set_params(&mut self, rate: f32, depth: f32, feedback: f32) {
        self.lfo.set_params(rate, 0.0, 1.0);
        self.depth = depth.clamp(0.0, 1.0);
        self.feedback = feedback.clamp(0.0, 0.95);
    }

    /// 처리
    pub fn process(&mut self, mut input: f32) -> f32 {
        // LFO로 올패스 필터 지연 변조
        let lfo_val = self.lfo.sine();
        // -1.0 ~ 1.0 -> 0.5ms ~ 5ms 범위
        let min_delay = self.sample_rate * 0.0005;
        let max_delay = self.sample_rate * 0.005;
        let delay = min_delay + (lfo_val + 1.0) * 0.5 * (max_delay - min_delay) * self.depth
            + min_delay * (1.0 - self.depth);

        self.ap1.set_params(delay, 0.5);
        self.ap2.set_params(delay * 1.3, -0.5);
        self.ap3.set_params(delay * 0.7, 0.5);
        self.ap4.set_params(delay * 1.7, -0.5);

        input += self.feedback * self.ap4.process(self.ap3.process(self.ap2.process(self.ap1.process(input))));

        let out = self.ap1.process(input) + self.ap2.process(input) + self.ap3.process(input) + self.ap4.process(input);
        out * 0.25
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.ap1.reset();
        self.ap2.reset();
        self.ap3.reset();
        self.ap4.reset();
        self.lfo.reset();
    }
}

impl Default for Phaser {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

/// Celeste/Detune 이펙트 (피치 시프트 합성)
#[derive(Debug, Clone)]
pub struct Celeste {
    /// 딜레이 라인 1
    buffer: Vec<f32>,
    pos: usize,
    /// 딜레이 시간 (초)
    delay: f32,
    /// 샘플 레이트
    sample_rate: f32,
}

impl Celeste {
    /// 새로운 Celeste 생성
    pub fn new(sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 0.05) as usize; // 50ms max
        Self {
            buffer: vec![0.0; max_delay],
            pos: 0,
            delay: 0.01, // 10ms 기본
            sample_rate,
        }
    }

    /// 딜레이 설정 (피치 차이를 위한 시간)
    /// detune_cents: 양수 = 약간 높은 음, 음수 = 약간 낮은 음
    pub fn set_detune(&mut self, detune_cents: f32) {
        // 100 cents = 반음, 1200 cents = 한 옥타브
        // detune = 1.0 + (detune_cents / 1200)
        let detune_ratio = (2.0_f32).powf(detune_cents / 1200.0);
        // 10ms 기준 딜레이
        self.delay = 0.01 * detune_ratio;
        self.delay = self.delay.clamp(0.001, 0.05);
    }

    /// 처리
    pub fn process(&mut self, input: f32) -> f32 {
        let read_pos = if self.pos as f32 >= self.delay * self.sample_rate {
            self.pos as f32 - self.delay * self.sample_rate
        } else {
            self.pos as f32 - self.delay * self.sample_rate + self.buffer.len() as f32
        };
        let int_pos = read_pos as usize;
        let frac = read_pos - int_pos as f32;
        let next_pos = (int_pos + 1) % self.buffer.len();

        let delayed = self.buffer[int_pos] * (1.0 - frac) + self.buffer[next_pos] * frac;
        self.buffer[self.pos] = input;
        self.pos = (self.pos + 1) % self.buffer.len();
        delayed * 0.5
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
    }
}

impl Default for Celeste {
    fn default() -> Self {
        Self::new(44100.0)
    }
}
