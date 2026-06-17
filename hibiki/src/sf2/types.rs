// sf2/types.rs - SF2 데이터 타입 정의

use std::sync::Arc;

/// SF2 파일 헤더
#[derive(Debug, Clone)]
pub struct SoundFontHeader {
    /// 사운드폰트 이름
    pub name: String,
    /// ROM 정보
    pub rom_name: String,
    /// ROM 버전
    pub rom_version: (u16, u16),
    /// 내부 버전
    pub internal_version: (u16, u16),
    /// 생성 소프트웨어
    pub software: String,
    /// 샘플 레이트 (기본값)
    pub sample_rate: u32,
}

/// 샘플 정보
/// 샘플 데이터는 Sf2File.smpl_data를 참조 (Arc로 공유)
#[derive(Debug, Clone)]
pub struct Sample {
    /// 샘플 이름
    pub name: String,
    /// 시작 인덱스 (샘플 데이터 내)
    pub start: u32,
    /// 종료 인덱스
    pub end: u32,
    /// 루프 시작
    pub start_loop: u32,
    /// 루프 종료
    pub end_loop: u32,
    /// 샘플 레이트
    pub sample_rate: u32,
    /// 원래 피치 (central note)
    pub original_pitch: u8,
    /// 피치 보정 (cents)
    pub pitch_correction: i8,
    /// 샘플 링크 (좌/우 채널용)
    pub sample_link: u16,
    /// 샘플 타입
    pub sample_type: SampleType,
}

impl Sample {
    /// 샘플의 실제 데이터 슬라이스를 가져오기 (Sf2File.smpl_data 필요)
    pub fn data_slice<'a>(&self, smpl_data: &'a [i16]) -> &'a [i16] {
        let start = self.start as usize;
        let end = (self.end as usize).min(smpl_data.len());
        &smpl_data[start..end]
    }
}

/// 샘플 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleType {
    Mono = 1,
    Right = 2,
    Left = 4,
    Linked = 8,
    RomMono = 0x8001,
    RomRight = 0x8002,
    RomLeft = 0x8004,
    RomLinked = 0x8008,
}

/// 악기 존 (프리셋과 샘플 사이의 매핑)
#[derive(Debug, Clone)]
pub struct InstrumentZone {
    /// 이 존의 샘플 인덱스
    pub sample_index: Option<usize>,
    /// 키 범위
    pub key_range: (u8, u8),
    /// 베élocity 범위
    pub velocity_range: (u8, u8),
    /// 제네레이터 파라미터
    pub generators: Vec<Generator>,
}

/// 악기 정의
#[derive(Debug, Clone)]
pub struct Instrument {
    /// 악기 이름
    pub name: String,
    /// 악기 존 목록
    pub zones: Vec<InstrumentZone>,
}

/// 프리셋 존
#[derive(Debug, Clone)]
pub struct PresetZone {
    /// 이 존의 악기 인덱스
    pub instrument_index: Option<usize>,
    /// 키 범위
    pub key_range: (u8, u8),
    /// 베élocity 범위
    pub velocity_range: (u8, u8),
    /// 제네레이터 파라미터
    pub generators: Vec<Generator>,
}

/// 프리셋 정의
#[derive(Debug, Clone)]
pub struct Preset {
    /// 프리셋 이름
    pub name: String,
    /// 뱅크 번호
    pub bank: u16,
    /// 프로그램 번호
    pub preset_num: u16,
    /// 프리셋 존
    pub zones: Vec<PresetZone>,
}

/// SF2 제네레이터 enum (스펙 기준 인덱스 번호만 정의)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum GeneratorType {
    // 인덱스 0-20
    StartAddressOffset = 0,
    EndAddressOffset = 1,
    StartAddressCoarseOffset = 2,
    ModLfoToPitch = 3,
    VibLfoToPitch = 4,
    ModEnvelopeToPitch = 5,
    InitialFilterFc = 6,
    InitialFilterQ = 7,
    ModLfoToFilterFc = 8,
    ModEnvelopeToFilterFc = 9,
    EndAddressCoarseOffset = 10,
    ModLfoToVolume = 11,
    Unused1 = 12,
    Unused2 = 13,
    ModLfoToFilterQ = 14,
    Unused3 = 15,
    Unused4 = 16,
    Pan = 17,
    Unused5 = 18,
    Unused6 = 19,
    Unused7 = 20,
    // 인덱스 21-35
    DelayModLfo = 21,
    FreqModLfo = 22,
    DelayVibLfo = 23,
    FreqVibLfo = 24,
    DelayModEnv = 25,
    AttackModEnv = 26,
    HoldModEnv = 27,
    DecayModEnv = 28,
    SustainModEnv = 29,
    ReleaseModEnv = 30,
    KeynumToModEnvHold = 31,
    KeynumToModEnvDecay = 32,
    DelayVolEnv = 33,
    AttackVolEnv = 34,
    HoldVolEnv = 35,
    DecayVolEnv = 36,
    SustainVolEnv = 37,
    ReleaseVolEnv = 38,
    KeynumToVolEnvHold = 39,
    KeynumToVolEnvDecay = 40,
    Instrument = 41,
    Reserved1 = 42,
    Reserved2 = 43,
    KeyRange = 44,
    VelocityRange = 45,
    StartAddressCoarseOffsetRight = 46,
    EndAddressCoarseOffsetRight = 47,
    StartAddressCoarseOffsetLeft = 48,
    EndAddressCoarseOffsetLeft = 49,
    Keynum = 50,
    Velocity = 51,
    InitialAttenuation = 52,
    Reserved3 = 53,
    Reserved4 = 54,
    EndLoopAddressCoarseOffset = 55,
    CoarseTune = 56,
    FineTune = 57,
    SampleId = 58,
    SampleMode = 59,
    Reserved5 = 60,
    ScaleTuning = 61,
    SampleExceptionCount = 62,
    ExclusiveClass = 63,
    OverrideRootKey = 64,
}

/// 제네레이터 파라미터 (범위 값)
#[derive(Debug, Clone)]
pub struct GeneratorRange {
    pub lo: u8,
    pub hi: u8,
}

/// 제네레이터 값
#[derive(Debug, Clone)]
pub enum GeneratorValue {
    /// 부호 없는 16비트 정수
    Uint16(u16),
    /// 부호 있는 16비트 정수
    Int16(i16),
    /// 범위 값
    Range(GeneratorRange),
}

/// 제네레이터 파라미터
#[derive(Debug, Clone)]
pub struct Generator {
    pub gen_type: GeneratorType,
    pub value: GeneratorValue,
}
