// hibiki.rs - Hibiki 사운드폰트 엔진
// 자체 사운드폰트 렌더링 엔진

pub mod sf2;
pub mod voice;
pub mod dsp;

// Hibiki 엔진 설정
pub struct HibikiSettings {
    sample_rate: f64,
    max_voices: usize,
}

impl HibikiSettings {
    // 새로운 설정 인스턴스 생성
    pub fn new() -> Self {
        Self {
            sample_rate: 44100.0,
            max_voices: 256,
        }
    }

    // 샘플 레이트 가져오기
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    // 샘플 레이트 설정
    pub fn set_sample_rate(&mut self, rate: f64) {
        self.sample_rate = rate;
    }

    // 최대 보이스 수 가져오기
    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    // 최대 보이스 수 설정
    pub fn set_max_voices(&mut self, voices: usize) {
        self.max_voices = voices;
    }
}

// Hibiki 사운드폰트 신디사이저
pub struct HibikiSynth {
    sample_rate: f64,
    gain: std::sync::RwLock<f32>,
    soundfont: std::sync::RwLock<Option<sf2::Sf2File>>,
    voice_manager: std::sync::RwLock<voice::VoiceManager>,
    // 채널 상태 (RwLock으로 보호해서 &self에서도 변경 가능)
    channels: std::sync::RwLock<[ChannelState; 16]>,
    // 이펙트
    chorus: std::sync::RwLock<dsp::Chorus>,
    reverb: std::sync::RwLock<dsp::Reverb>,
    delay: std::sync::RwLock<dsp::Delay>,
    tremolo: std::sync::RwLock<dsp::Tremolo>,
    phaser: std::sync::RwLock<dsp::Phaser>,
    celeste: std::sync::RwLock<dsp::Celeste>,
    // 이펙트 활성화
    effect_enabled: std::sync::RwLock<EffectEnable>,
}

/// 이펙트 활성화 상태
#[derive(Debug, Clone, Copy)]
struct EffectEnable {
    reverb: bool,
    chorus: bool,
    delay: bool,
    tremolo: bool,
    phaser: bool,
    celeste: bool,
}

impl Default for EffectEnable {
    fn default() -> Self {
        Self {
            reverb: true,
            chorus: true,
            delay: false,
            tremolo: false,
            phaser: false,
            celeste: false,
        }
    }
}

/// RPN/NRPN 상태
#[derive(Debug, Clone, Copy)]
enum RpnState {
    None,
    Rpn(u16),
    Nrpn(u16),
}

/// 채널 상태 (확장)
#[derive(Debug, Clone, Copy)]
struct ChannelState {
    // 기본
    program: u8,
    bank: u16,
    volume: u8,
    pan: u8,
    expression: u8,
    // 피치
    pitch_bend: i16,
    pitch_bend_sens: u8,
    // 모듈레이션
    modulation: u8,
    // 포르타멘토
    portamento_time: u8,
    portamento_on: bool,
    portamento_control: u8,
    last_note: u8,
    // 이펙트 (GS/XG)
    reverb_send: u8,
    chorus_send: u8,
    // GS 확장
    scale_tune: u8,
    // RPN/NRPN
    rpn_state: RpnState,
    // GS RPN
    vibrato_rate: u8,
    vibrato_depth: u8,
    vibrato_delay: u8,
    // XG
    filter_cutoff: u8,
    filter_resonance: u8,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            program: 0,
            bank: 0,
            volume: 100,
            pan: 64,
            expression: 127,
            pitch_bend: 0,
            pitch_bend_sens: 2,
            modulation: 0,
            portamento_time: 0,
            portamento_on: false,
            portamento_control: 0,
            last_note: 60,
            reverb_send: 40,
            chorus_send: 0,
            scale_tune: 0,
            rpn_state: RpnState::None,
            vibrato_rate: 64,
            vibrato_depth: 64,
            vibrato_delay: 64,
            filter_cutoff: 127,
            filter_resonance: 0,
        }
    }
}

impl HibikiSynth {
    // 새로운 신디사이저 인스턴스 생성
    pub fn new(settings: HibikiSettings) -> Result<Self, String> {
        let sample_rate = settings.sample_rate;

        Ok(Self {
            sample_rate,
            gain: std::sync::RwLock::new(1.0),
            soundfont: std::sync::RwLock::new(None),
            voice_manager: std::sync::RwLock::new(voice::VoiceManager::new(settings.max_voices)),
            channels: std::sync::RwLock::new([ChannelState::default(); 16]),
            chorus: std::sync::RwLock::new(dsp::Chorus::new(sample_rate as f32)),
            reverb: std::sync::RwLock::new(dsp::Reverb::new(sample_rate as f32)),
            delay: std::sync::RwLock::new(dsp::Delay::new(sample_rate as f32)),
            tremolo: std::sync::RwLock::new(dsp::Tremolo::new(sample_rate as f32)),
            phaser: std::sync::RwLock::new(dsp::Phaser::new(sample_rate as f32)),
            celeste: std::sync::RwLock::new(dsp::Celeste::new(sample_rate as f32)),
            effect_enabled: std::sync::RwLock::new(EffectEnable::default()),
        })
    }

    // 사운드폰트 로드
    pub fn sfload(&self, path: &str, _reset_presets: bool) -> Result<u32, String> {
        // 파일 열기
        let mut file = std::fs::File::open(path)
            .map_err(|e| format!("Failed to open file: {}", e))?;

        // SF2 파싱
        let sf2_file = sf2::Sf2Parser::parse(&mut file)
            .map_err(|e| format!("Failed to parse SF2: {:?}", e))?;

        // 프리셋 개수 반환
        let preset_count = sf2_file.presets.len() as u32;

        // 사운드폰트 저장
        *self.soundfont.write().unwrap() = Some(sf2_file);

        // 보이스 초기화
        self.voice_manager.write().unwrap().reset();

        Ok(preset_count)
    }

    // 사운드폰트 언로드
    pub fn sfunload(&self) {
        *self.soundfont.write().unwrap() = None;
        self.voice_manager.write().unwrap().reset();
    }

    // 게인 설정
    pub fn set_gain(&self, gain: f32) {
        *self.gain.write().unwrap() = gain.clamp(0.0, 10.0);
    }

    // 게인 가져오기
    pub fn get_gain(&self) -> f32 {
        *self.gain.read().unwrap()
    }

    // 이펙트 설정
    pub fn set_effect_level(&self, reverb: u8, chorus: u8) {
        // 0-127 -> 0.0-1.0
        self.reverb.write().unwrap().set_params(reverb as f32 / 127.0, 0.5);
        self.chorus.write().unwrap().set_params(1.0 + chorus as f32 / 127.0 * 2.0, chorus as f32 / 127.0, 25.0);
    }

    // 노트 온 (음 발생)
    pub fn note_on(&self, channel: u32, note: u32, velocity: u32) -> Result<(), String> {
        if channel >= 16 || note > 127 || velocity > 127 {
            return Err("Invalid parameter".to_string());
        }

        let channel = channel as usize;
        let note = note as u8;
        let velocity = velocity as u8;

        if velocity == 0 {
            // velocity 0은 note off로 처리
            return self.note_off(channel as u32, note as u32);
        }

        let sf = self.soundfont.read().unwrap();
        let sf = sf.as_ref().ok_or("No soundfont loaded")?;

        // 채널 상태 읽기
        let ch_state = self.channels.read().unwrap()[channel];

        // 뱅크 셀렉트 처리
        let bank = ch_state.bank;
        let program = ch_state.program as u16;

        // 프리셋 찾기
        let preset = sf.presets.iter()
            .find(|p| p.bank == bank && p.preset_num == program)
            .or_else(|| sf.presets.iter().find(|p| p.bank == 0 && p.preset_num == program))
            .or_else(|| sf.presets.iter().find(|p| p.bank == bank && p.preset_num == 0))
            .or_else(|| sf.presets.iter().find(|p| p.bank == 128 && p.preset_num == program))
            .ok_or("Preset not found")?;

        // 노트에 맞는 악기 존 찾기
        for zone in &preset.zones {
            let (key_lo, key_hi) = zone.key_range;
            let (vel_lo, vel_hi) = zone.velocity_range;

            if note >= key_lo && note <= key_hi
                && velocity >= vel_lo && velocity <= vel_hi
            {
                if let Some(inst_idx) = zone.instrument_index {
                    if inst_idx >= sf.instruments.len() {
                        continue;
                    }
                    let instrument = &sf.instruments[inst_idx];

                    // 악기에서 샘플 찾기
                    for inst_zone in &instrument.zones {
                        if let Some(sample_idx) = inst_zone.sample_index {
                            if sample_idx >= sf.samples.len() {
                                continue;
                            }
                            let sample = &sf.samples[sample_idx];

                            // 보이스 트리거
                            let mut vm = self.voice_manager.write().unwrap();
                            if let Some(voice) = vm.find_free_voice() {
                                voice.trigger(sample, sf.smpl_data.clone(), note, velocity, self.sample_rate as f32);

                                // 채널 설정 적용
                                voice.pan = ch_state.pan as f32 / 127.0;
                                // 볼륨: CC7 * CC11 (expression) / 127^2
                                let vol = (ch_state.volume as f32 / 127.0)
                                    * (ch_state.expression as f32 / 127.0);
                                voice.set_volume(vol);

                                // 피치벤드 적용
                                let bend = ch_state.pitch_bend as f32;
                                let sens = ch_state.pitch_bend_sens as f32;
                                voice.apply_pitch_bend(bend, sens);

                                // 모듈레이션 (LFO)
                                let mod_depth = ch_state.modulation as f32 / 127.0;
                                voice.set_modulation_depth(mod_depth);

                                // 필터 설정 (CC71:resonance, CC74:cutoff)
                                let cutoff = ch_state.filter_cutoff as f32;
                                let resonance = ch_state.filter_resonance as f32;
                                voice.set_filter(cutoff, resonance);

                                // 비브라토 설정 (CC76)
                                let vib_depth = ch_state.vibrato_depth as f32;
                                voice.set_vibrato_depth(vib_depth);
                            }
                            return Ok(());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // 노트 오프 (음 정지)
    pub fn note_off(&self, channel: u32, note: u32) -> Result<(), String> {
        if channel >= 16 || note > 127 {
            return Err("Invalid parameter".to_string());
        }

        // 모든 관련 보이스를 릴리즈
        let mut vm = self.voice_manager.write().unwrap();
        for voice in &mut vm.voices {
            // TODO: 채널, 노트 매칭 확인
            if voice.state != voice::VoiceState::Off {
                voice.release();
            }
        }

        Ok(())
    }

    // 컨트롤러 변경 (확장)
    pub fn cc(&self, channel: u32, controller: u32, value: u32) -> Result<(), String> {
        if channel >= 16 || controller > 127 || value > 127 {
            return Err("Invalid parameter".to_string());
        }

        let channel = channel as usize;
        let controller = controller as u8;
        let value = value as u8;

        // RPN/NRPN 상태 확인
        let rpn_state = self.channels.read().unwrap()[channel].rpn_state;
        match rpn_state {
            RpnState::Rpn(rpn) => {
                match rpn {
                    0 => {
                        // Pitch Bend Sensitivity
                        self.channels.write().unwrap()[channel].pitch_bend_sens = value;
                    }
                    1 => {
                        // Fine Tuning (단위: cents, 8192 = 0 cents)
                    }
                    2 => {
                        // Coarse Tuning (단위: semitones, 8192 = 0)
                    }
                    4 => {
                        // Channel Volume LSB (거의 사용 안 함)
                    }
                    _ => {}
                }
                self.channels.write().unwrap()[channel].rpn_state = RpnState::None;
                return Ok(());
            }
            RpnState::Nrpn(_) => {
                // NRPN 처리 (XG 확장)
                self.channels.write().unwrap()[channel].rpn_state = RpnState::None;
                return Ok(());
            }
            RpnState::None => {}
        }

        match controller {
            // 기본 CC
            0 => {
                // 뱅크 셀렉트 MSB
                let mut chs = self.channels.write().unwrap();
                chs[channel].bank = (chs[channel].bank & 0x7F) | ((value as u16) << 7);
            }
            1 => {
                // 모듈레이션 (모드 체인저)
                self.channels.write().unwrap()[channel].modulation = value;
            }
            5 => {
                // 포르타멘토 시간
                self.channels.write().unwrap()[channel].portamento_time = value;
            }
            6 => {
                // Data Entry MSB
            }
            7 => {
                // 볼륨
                self.channels.write().unwrap()[channel].volume = value;
            }
            10 => {
                // 패너
                self.channels.write().unwrap()[channel].pan = value;
            }
            11 => {
                // 표현력
                self.channels.write().unwrap()[channel].expression = value;
            }
            32 => {
                // 뱅크 셀렉트 LSB
                let mut chs = self.channels.write().unwrap();
                chs[channel].bank = (chs[channel].bank & 0x3F80) | (value as u16);
            }
            37 => {
                // Data Entry LSB
            }
            38 => {
                // Data Entry LSB (NRPN용)
            }
            64 => {
                // Sustain (Damper) Pedal
                if value >= 64 {
                    // Sustain on
                } else {
                    // Sustain off - 모든 음 릴리즈
                    let mut vm = self.voice_manager.write().unwrap();
                    for voice in &mut vm.voices {
                        voice.release();
                    }
                }
            }
            65 => {
                // 포르타멘토 On/Off
                self.channels.write().unwrap()[channel].portamento_on = value >= 64;
            }
            71 => {
                // 필터 공진 (GS)
                self.channels.write().unwrap()[channel].filter_resonance = value;
                self.apply_filter_to_channel(channel);
            }
            72 => {
                // 필터 컷오프 해제 시간
            }
            73 => {
                // 공격 시간
            }
            74 => {
                // 필터 컷오프 (GS/XG)
                self.channels.write().unwrap()[channel].filter_cutoff = value;
                self.apply_filter_to_channel(channel);
            }
            75 => {
                // 밝기
            }
            76 => {
                // vibrato depth (GS)
                self.channels.write().unwrap()[channel].vibrato_depth = value;
                self.apply_vibrato_to_channel(channel);
            }
            77 => {
                // channel mode (GS)
            }
            78 => {
                // 밝기 (GS)
            }
            84 => {
                // 포르타멘토 control
                self.channels.write().unwrap()[channel].portamento_control = value;
            }
            91 => {
                // Reverb Send Level
                self.channels.write().unwrap()[channel].reverb_send = value;
                self.reverb.write().unwrap().set_params(value as f32 / 127.0, 0.5);
            }
            92 => {
                // Tremolo (CC92: 0-127 -> 트레몰로 깊이)
                self.tremolo.write().unwrap().set_params(5.0, value as f32 / 127.0);
                self.effect_enabled.write().unwrap().tremolo = value > 0;
            }
            93 => {
                // Chorus Send Level
                self.channels.write().unwrap()[channel].chorus_send = value;
                self.chorus.write().unwrap().set_params(
                    1.0 + value as f32 / 127.0 * 2.0,
                    value as f32 / 127.0,
                    25.0,
                );
            }
            94 => {
                // Celeste/Detune (GS)
                let cents = (value as f32 - 64.0) * 0.78;
                self.celeste.write().unwrap().set_detune(cents);
                self.effect_enabled.write().unwrap().celeste = value != 64;
            }
            95 => {
                // 페이저 (GS)
                self.phaser.write().unwrap().set_params(0.5, value as f32 / 127.0, 0.3);
                self.effect_enabled.write().unwrap().phaser = value > 0;
            }
            96 => {
                // Data Increment
            }
            97 => {
                // Data Decrement
            }
            98 => {
                // NRPN LSB
                self.channels.write().unwrap()[channel].rpn_state = RpnState::Nrpn(value as u16);
            }
            99 => {
                // NRPN MSB
                let mut chs = self.channels.write().unwrap();
                let nrpn = chs[channel].rpn_state;
                match nrpn {
                    RpnState::Nrpn(lsb) => {
                        chs[channel].rpn_state = RpnState::Nrpn((value as u16) << 7 | lsb);
                    }
                    _ => {
                        chs[channel].rpn_state = RpnState::Nrpn(value as u16);
                    }
                }
            }
            100 => {
                // RPN LSB
                let mut chs = self.channels.write().unwrap();
                match value {
                    127 => chs[channel].rpn_state = RpnState::None, // Reset
                    _ => {
                        let rpn = chs[channel].rpn_state;
                        match rpn {
                            RpnState::Rpn(msb) => {
                                chs[channel].rpn_state = RpnState::Rpn((msb << 7) | value as u16);
                            }
                            _ => {
                                chs[channel].rpn_state = RpnState::Rpn(value as u16);
                            }
                        }
                    }
                }
            }
            101 => {
                // RPN MSB
                let mut chs = self.channels.write().unwrap();
                match value {
                    127 => chs[channel].rpn_state = RpnState::None, // Reset All
                    _ => {
                        chs[channel].rpn_state = RpnState::Rpn((value as u16) << 7);
                    }
                }
            }
            120 => {
                // All Sound Off
                let mut vm = self.voice_manager.write().unwrap();
                vm.reset();
            }
            121 => {
                // Reset Controllers
                self.channels.write().unwrap()[channel] = ChannelState::default();
            }
            123 => {
                // All Notes Off
                let mut vm = self.voice_manager.write().unwrap();
                for voice in &mut vm.voices {
                    voice.release();
                }
            }
            124 => {
                // Omni Off
            }
            125 => {
                // Omni On
            }
            126 => {
                // Mono Mode (Poly Off)
            }
            127 => {
                // Poly Mode (Mono Off)
            }
            _ => {}
        }

        Ok(())
    }

    // 프로그램 변경 (악기 변경)
    pub fn program_change(&self, channel: u32, program: u32) -> Result<(), String> {
        if channel >= 16 || program > 127 {
            return Err("Invalid parameter".to_string());
        }

        self.channels.write().unwrap()[channel as usize].program = program as u8;
        Ok(())
    }

    // 피치 벤드
    pub fn pitch_bend(&self, channel: u32, value: u32) -> Result<(), String> {
        if channel >= 16 || value > 16383 {
            return Err("Invalid parameter".to_string());
        }

        // MIDI 피치벤드: 0-16383, 중앙 8192
        let bend = value as i32 - 8192;
        self.channels.write().unwrap()[channel as usize].pitch_bend = bend as i16;
        Ok(())
    }

    // 피치 벤드 감도 설정
    pub fn pitch_wheel_sens(&self, channel: u32, value: u32) -> Result<(), String> {
        if channel >= 16 || value > 24 {
            return Err("Invalid parameter".to_string());
        }

        self.channels.write().unwrap()[channel as usize].pitch_bend_sens = value as u8;
        Ok(())
    }

    // 뱅크 셀렉트
    pub fn bank_select(&self, channel: u32, bank: u32) -> Result<(), String> {
        if channel >= 16 || bank > 16383 {
            return Err("Invalid parameter".to_string());
        }

        self.channels.write().unwrap()[channel as usize].bank = bank as u16;
        Ok(())
    }

    // 시스템 리셋
    pub fn system_reset(&self) -> Result<(), String> {
        let mut chs = self.channels.write().unwrap();
        for i in 0..16 {
            chs[i] = ChannelState::default();
        }
        drop(chs);
        self.voice_manager.write().unwrap().reset();
        self.chorus.write().unwrap().reset();
        self.reverb.write().unwrap().reset();
        self.delay.write().unwrap().reset();
        self.tremolo.write().unwrap().reset();
        self.phaser.write().unwrap().reset();
        self.celeste.write().unwrap().reset();
        Ok(())
    }

    // 샘플 버퍼에 오디오 데이터 쓰기
    pub fn write_samples(&self, output: &mut [f32; 2]) -> Result<(), String> {
        let gain = *self.gain.read().unwrap();

        // 모든 보이스 렌더링
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        let mut vm = self.voice_manager.write().unwrap();
        for voice in &mut vm.voices {
            if voice.state != voice::VoiceState::Off {
                let (l, r) = voice.render_sample();
                left += l;
                right += r;
            }
        }

        // 이펙트 적용 (후에 추가)
        // left = self.chorus.process(left);
        // left = self.reverb.process(left);
        // right = self.chorus.process(right);
        // right = self.reverb.process(right);

        // 게인 적용
        left *= gain;
        right *= gain;

        // 클리핑
        output[0] = left.clamp(-1.0, 1.0);
        output[1] = right.clamp(-1.0, 1.0);

        Ok(())
    }

    // 샘플 버퍼에 오디오 데이터 쓰기 (이펙트 포함)
    pub fn write_samples_with_effects(&self, output: &mut [f32; 2]) -> Result<(), String> {
        let gain = *self.gain.read().unwrap();

        // 모든 보이스 렌더링
        let mut left = 0.0f32;
        let mut right = 0.0f32;

        let mut vm = self.voice_manager.write().unwrap();
        for voice in &mut vm.voices {
            if voice.state != voice::VoiceState::Off {
                let (l, r) = voice.render_sample();
                left += l;
                right += r;
            }
        }

        // 이펙트 적용
        let ee = *self.effect_enabled.read().unwrap();
        if ee.tremolo {
            left = self.tremolo.write().unwrap().process(left);
            right = self.tremolo.write().unwrap().process(right);
        }
        if ee.celeste {
            left += self.celeste.write().unwrap().process(left);
            right += self.celeste.write().unwrap().process(right);
        }
        if ee.phaser {
            left = self.phaser.write().unwrap().process(left);
            right = self.phaser.write().unwrap().process(right);
        }
        if ee.chorus {
            let (l, r) = self.chorus.write().unwrap().process_stereo(left, right);
            left = l;
            right = r;
        }
        if ee.reverb {
            let (l, r) = self.reverb.write().unwrap().process_stereo(left, right);
            left = l;
            right = r;
        }

        // 게인 적용
        left *= gain;
        right *= gain;

        // 클리핑
        output[0] = left.clamp(-1.0, 1.0);
        output[1] = right.clamp(-1.0, 1.0);

        Ok(())
    }

    // 로드된 사운드폰트 정보 가져오기
    pub fn get_soundfont_info(&self) -> Option<sf2::Sf2File> {
        self.soundfont.read().unwrap().clone()
    }

    // 활성 보이스 수 가져오기
    pub fn active_voices(&self) -> usize {
        let vm = self.voice_manager.read().unwrap();
        vm.active_count()
    }

    // 이펙트 활성화/비활성화
    pub fn enable_effect(&self, reverb: bool, chorus: bool, delay: bool) {
        let mut ee = self.effect_enabled.write().unwrap();
        ee.reverb = reverb;
        ee.chorus = chorus;
        ee.delay = delay;
    }

    // 트레몰로 활성화
    pub fn enable_tremolo(&self, on: bool) {
        self.effect_enabled.write().unwrap().tremolo = on;
    }

    // 페이저 활성화
    pub fn enable_phaser(&self, on: bool) {
        self.effect_enabled.write().unwrap().phaser = on;
    }

    // Celeste 활성화
    pub fn enable_celeste(&self, on: bool) {
        self.effect_enabled.write().unwrap().celeste = on;
    }

    // 트레몰로 파라미터 설정
    pub fn set_tremolo_params(&self, rate: f32, depth: f32) {
        self.tremolo.write().unwrap().set_params(rate, depth);
    }

    // 페이저 파라미터 설정
    pub fn set_phaser_params(&self, rate: f32, depth: f32, feedback: f32) {
        self.phaser.write().unwrap().set_params(rate, depth, feedback);
    }

    // Celeste detune 설정 (cents)
    pub fn set_celeste_detune(&self, cents: f32) {
        self.celeste.write().unwrap().set_detune(cents);
    }

    // 특정 채널의 활성 보이스에 필터 적용
    fn apply_filter_to_channel(&self, channel: usize) {
        let (cutoff, resonance) = {
            let chs = self.channels.read().unwrap();
            (chs[channel].filter_cutoff as f32, chs[channel].filter_resonance as f32)
        };
        let mut vm = self.voice_manager.write().unwrap();
        for voice in &mut vm.voices {
            if voice.state != voice::VoiceState::Off {
                voice.set_filter(cutoff, resonance);
            }
        }
    }

    // 특정 채널의 활성 보이스에 비브라토 적용
    fn apply_vibrato_to_channel(&self, channel: usize) {
        let depth = self.channels.read().unwrap()[channel].vibrato_depth as f32;
        let mut vm = self.voice_manager.write().unwrap();
        for voice in &mut vm.voices {
            if voice.state != voice::VoiceState::Off {
                voice.set_vibrato_depth(depth);
            }
        }
    }

    // 코러스 파라미터 설정
    pub fn set_chorus_params(&self, rate: f32, depth: f32, delay_ms: f32) {
        self.chorus.write().unwrap().set_params(rate, depth, delay_ms);
    }

    // 리버브 파라미터 설정
    pub fn set_reverb_params(&self, size: f32, damping: f32) {
        self.reverb.write().unwrap().set_params(size, damping);
    }
}

// Hibiki 로그 핸들러
pub struct HibikiLogger;

impl HibikiLogger {
    pub fn new<F>(_callback: F) -> Self
    where
        F: Fn(u32, &str) + Send + 'static,
    {
        Self
    }
}

// Hibiki 로그 레벨
pub mod log_level {
    pub const PANIC: u32 = 1;
    pub const ERROR: u32 = 2;
    pub const WARNING: u32 = 3;
    pub const INFO: u32 = 4;
    pub const DEBUG: u32 = 5;
}

// 로그 레벨 설정
pub fn set_log_levels(_levels: &[u32], _handler: HibikiLogger) {
    // TODO: 실제 로깅 구현
}
