// sf2_oxi/adapter.rs - SoundFont2 → 우리 Sf2File 어댑터

use std::sync::Arc;

use super::{SoundFont2, SampleHeader};

use crate::sf2_oxi::raw::SampleChunk;

/// 우리 Sf2File (음악 엔진이 사용하는 구조체)
#[derive(Debug, Clone)]
pub struct Sf2File {
    /// 사운드폰트 이름
    pub name: String,
    /// 사운드폰트 샘플 데이터 (i16)
    pub smpl_data: Arc<Vec<i16>>,
    /// 샘플 헤더들
    pub samples: Vec<Sample>,
    /// 프리셋들
    pub presets: Vec<Preset>,
    /// 악기들
    pub instruments: Vec<Instrument>,
}

/// 샘플 정보
#[derive(Debug, Clone)]
pub struct Sample {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub start_loop: u32,
    pub end_loop: u32,
    pub sample_rate: u32,
    pub original_pitch: u8,
    pub pitch_correction: i8,
    pub sample_link: u16,
    pub sample_type: SampleType,
}

impl Sample {
    pub fn data_slice<'a>(&self, smpl_data: &'a [i16]) -> &'a [i16] {
        let start = self.start as usize;
        let end = (self.end as usize).min(smpl_data.len());
        &smpl_data[start..end]
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SampleType {
    Mono,
    Right,
    Left,
    Linked,
    RomMono,
    RomRight,
    RomLeft,
    RomLinked,
}

/// 프리셋
#[derive(Debug, Clone)]
pub struct Preset {
    pub name: String,
    pub bank: u16,
    pub preset_num: u16,
    pub zones: Vec<PresetZone>,
}

#[derive(Debug, Clone)]
pub struct PresetZone {
    pub instrument_index: Option<usize>,
    pub key_range: (u8, u8),
    pub velocity_range: (u8, u8),
}

/// 악기
#[derive(Debug, Clone)]
pub struct Instrument {
    pub name: String,
    pub zones: Vec<InstrumentZone>,
}

/// 악기 zone - SF2 generator 정보 포함
#[derive(Debug, Clone)]
pub struct InstrumentZone {
    pub sample_index: Option<usize>,
    pub key_range: (u8, u8),
    pub velocity_range: (u8, u8),
    /// Attack time (seconds)
    pub attack: f32,
    /// Decay time (seconds)
    pub decay: f32,
    /// Sustain level (0.0~1.0)
    pub sustain: f32,
    /// Release time (seconds)
    pub release: f32,
    /// Initial attenuation (centibel, 0 = no attenuation)
    pub attenuation: f32,
}

impl Sf2File {
    /// OxiSynth SoundFont2에서 변환
    pub fn from_oxi(sf: SoundFont2, smpl_chunk: Option<SampleChunk>, file: &mut std::fs::File) -> Result<Self, String> {
        use std::io::{Read, Seek};

        // smpl 데이터 읽기
        let smpl_data = if let Some(smpl) = smpl_chunk {
            use std::io::SeekFrom;
            file.seek(SeekFrom::Start(smpl.offset))
                .map_err(|e| format!("seek: {}", e))?;
            let mut buf = vec![0u8; smpl.len as usize];
            file.read_exact(&mut buf).map_err(|e| format!("read: {}", e));

            // i16으로 변환
            let mut samples_i16: Vec<i16> = Vec::with_capacity(buf.len() / 2);
            let mut i = 0;
            while i + 2 <= buf.len() {
                let s = i16::from_le_bytes([buf[i], buf[i + 1]]);
                samples_i16.push(s);
                i += 2;
            }
            Arc::new(samples_i16)
        } else {
            Arc::new(Vec::new())
        };

        // 샘플 변환
        let samples: Vec<Sample> = sf.sample_headers.iter().map(|s| Sample {
            name: s.name.clone(),
            start: s.start,
            end: s.end,
            start_loop: s.loop_start,
            end_loop: s.loop_end,
            sample_rate: s.sample_rate,
            original_pitch: s.origpitch,
            pitch_correction: s.pitchadj,
            sample_link: s.sample_link,
            sample_type: sample_type_to_oxi(s.sample_type),
        }).collect();

        // 악기 변환
        let instruments: Vec<Instrument> = sf.instruments.iter().map(|i| {
            // global zone (sample=None) 또는 첫 sample zone의 envelope을 instrument 기본값으로 사용
            let mut default_attack: f32 = 0.001;
            let mut default_decay: f32 = 0.001;
            let mut default_sustain: f32 = 1.0;
            let mut default_release: f32 = 0.05;
            let mut default_attenuation: f32 = 0.0;

            // 모든 zone의 generator를 누적 (각 zone은 global zone의 값을 override)
            for z in &i.zones {
                for g in &z.gen_list {
                    let ty = match &g.ty {
                        crate::sf2_oxi::SfEnum::Value(t) => Some(*t),
                        crate::sf2_oxi::SfEnum::Unknown(_) => None,
                    };
                    let Some(ty) = ty else { continue };
                    if let Some(v) = g.amount.as_i16() {
                        match ty {
                            crate::sf2_oxi::GeneratorType::AttackVolEnv => {
                                default_attack = timecents_to_seconds(*v);
                            }
                            crate::sf2_oxi::GeneratorType::DecayVolEnv => {
                                default_decay = timecents_to_seconds(*v);
                            }
                            crate::sf2_oxi::GeneratorType::SustainVolEnv => {
                                default_sustain = (*v as f32).clamp(0.0, 1000.0) / 1000.0;
                            }
                            crate::sf2_oxi::GeneratorType::ReleaseVolEnv => {
                                default_release = timecents_to_seconds(*v);
                            }
                            crate::sf2_oxi::GeneratorType::InitialAttenuation => {
                                default_attenuation = *v as f32;
                            }
                            _ => {}
                        }
                    }
                }
            }

            let zones: Vec<InstrumentZone> = i.zones.iter().map(|z| {
                let sample_index = z.sample().map(|&x| x as usize);
                let key_range = z.key_range()
                    .map(|&v| ((v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8))
                    .unwrap_or((0, 127));
                let vel_range = z.vel_range()
                    .map(|r| (r.low, r.high))
                    .unwrap_or((0, 127));

                // zone-specific envelope 값 (있으면 override, 없으면 instrument default)
                let mut attack = default_attack;
                let mut decay = default_decay;
                let mut sustain = default_sustain;
                let mut release = default_release;
                let mut attenuation = default_attenuation;
                for g in &z.gen_list {
                    let ty = match &g.ty {
                        crate::sf2_oxi::SfEnum::Value(t) => Some(*t),
                        crate::sf2_oxi::SfEnum::Unknown(_) => None,
                    };
                    let Some(ty) = ty else { continue };
                    if let Some(v) = g.amount.as_i16() {
                        match ty {
                            crate::sf2_oxi::GeneratorType::AttackVolEnv => {
                                attack = timecents_to_seconds(*v);
                            }
                            crate::sf2_oxi::GeneratorType::DecayVolEnv => {
                                decay = timecents_to_seconds(*v);
                            }
                            crate::sf2_oxi::GeneratorType::SustainVolEnv => {
                                sustain = (*v as f32).clamp(0.0, 1000.0) / 1000.0;
                            }
                            crate::sf2_oxi::GeneratorType::ReleaseVolEnv => {
                                release = timecents_to_seconds(*v);
                            }
                            crate::sf2_oxi::GeneratorType::InitialAttenuation => {
                                attenuation = *v as f32;
                            }
                            _ => {}
                        }
                    }
                }

                InstrumentZone {
                    sample_index,
                    key_range,
                    velocity_range: vel_range,
                    attack,
                    decay,
                    sustain,
                    release,
                    attenuation,
                }
            }).collect();
            Instrument {
                name: i.header.name.clone(),
                zones,
            }
        }).collect();

        // 프리셋 변환
        let presets: Vec<Preset> = sf.presets.iter().map(|p| {
            let zones: Vec<PresetZone> = p.zones.iter().map(|z| {
                let instrument_index = z.instrument().map(|&x| x as usize);
                let key_range = z.key_range()
                    .map(|&v| ((v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8))
                    .unwrap_or((0, 127));
                let vel_range = z.vel_range()
                    .map(|r| (r.low, r.high))
                    .unwrap_or((0, 127));
                PresetZone {
                    instrument_index,
                    key_range,
                    velocity_range: vel_range,
                }
            }).collect();
            Preset {
                name: p.header.name.clone(),
                bank: p.header.bank,
                preset_num: p.header.preset,
                zones,
            }
        }).collect();

        Ok(Sf2File {
            name: sf.info.bank_name,
            smpl_data,
            samples,
            instruments,
            presets,
        })
    }
}

fn sample_type_to_oxi(t: super::SampleLink) -> SampleType {
    use super::SampleLink as SL;
    match t {
        SL::None => SampleType::Mono,
        SL::MonoSample => SampleType::Mono,
        SL::RightSample => SampleType::Right,
        SL::LeftSample => SampleType::Left,
        SL::LinkedSample => SampleType::Linked,
        SL::RomMonoSample => SampleType::RomMono,
        SL::RomRightSample => SampleType::RomRight,
        SL::RomLeftSample => SampleType::RomLeft,
        SL::RomLinkedSample => SampleType::RomLinked,
        SL::VorbisMonoSample => SampleType::Mono,
        SL::VorbisRightSample => SampleType::Right,
        SL::VorbisLeftSample => SampleType::Left,
        SL::VorbisLinkedSample => SampleType::Linked,
    }
}

/// SF2 timecents → seconds
/// timecents: 1200 = 2초, 0 = 1초, -1200 = 0.5초, -32768 = 0.001초
fn timecents_to_seconds(timecents: i16) -> f32 {
    if timecents <= -32768 {
        return 0.001;
    }
    2f32.powf(timecents as f32 / 1200.0)
}
