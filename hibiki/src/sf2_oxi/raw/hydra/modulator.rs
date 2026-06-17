// raw/hydra/modulator.rs
// OxiSynth의 soundfont-rs modulator.rs를 vendoring

use super::utils::Reader;
use crate::sf2_oxi::error::Error;
use crate::sf2_oxi::riff::{Chunk, ChunkId, ScratchReader};
use std::io::{Read, Seek};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeneralPalette {
    NoController,
    NoteOnVelocity,
    NoteOnKeyNumber,
    PolyPressure,
    ChannelPressure,
    PitchWheel,
    PitchWheelSensitivity,
    Link,
    Unknown(u8),
}

impl From<u8> for GeneralPalette {
    fn from(ty: u8) -> Self {
        match ty {
            0 => Self::NoController,
            2 => Self::NoteOnVelocity,
            3 => Self::NoteOnKeyNumber,
            10 => Self::PolyPressure,
            13 => Self::ChannelPressure,
            14 => Self::PitchWheel,
            16 => Self::PitchWheelSensitivity,
            127 => Self::Link,
            v => Self::Unknown(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControllerPalette {
    NoController,
    NoteOnVelocity,
    NoteOnKeyNumber,
    PolyPressure,
    ChannelPressure,
    PitchWheel,
    PitchWheelSensitivity,
    Link,
    Unknown(u8),
}

impl From<GeneralPalette> for ControllerPalette {
    fn from(g: GeneralPalette) -> Self {
        match g {
            GeneralPalette::NoController => Self::NoController,
            GeneralPalette::NoteOnVelocity => Self::NoteOnVelocity,
            GeneralPalette::NoteOnKeyNumber => Self::NoteOnKeyNumber,
            GeneralPalette::PolyPressure => Self::PolyPressure,
            GeneralPalette::ChannelPressure => Self::ChannelPressure,
            GeneralPalette::PitchWheel => Self::PitchWheel,
            GeneralPalette::PitchWheelSensitivity => Self::PitchWheelSensitivity,
            GeneralPalette::Link => Self::Link,
            GeneralPalette::Unknown(v) => Self::Unknown(v),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceDirection {
    Positive,
    Negative,
    Unknown(bool),
}

impl From<bool> for SourceDirection {
    fn from(d: bool) -> Self {
        match d {
            true => Self::Positive,
            false => Self::Negative,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourcePolarity {
    Unipolar,
    Bipolar,
    Unknown(bool),
}

impl From<bool> for SourcePolarity {
    fn from(p: bool) -> Self {
        match p {
            true => Self::Bipolar,
            false => Self::Unipolar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceType {
    General,
    Controller,
    Unknown(bool),
}

impl From<bool> for SourceType {
    fn from(t: bool) -> Self {
        match t {
            true => Self::Controller,
            false => Self::General,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModulatorSource {
    pub ctrl: ControllerPalette,
    pub direction: SourceDirection,
    pub polarity: SourcePolarity,
    pub typ: SourceType,
    pub index: u16,
}

impl From<u16> for ModulatorSource {
    fn from(src: u16) -> Self {
        // SF2 Modulator Source encoding:
        // 0-6 bits: index (CC index)
        // 7 bit: direction (0=negative, 1=positive)
        // 8 bit: polarity (0=unipolar, 1=bipolar)
        // 9 bit: type (0=general, 1=controller)
        // 10-15 bits: controller palette
        let index = (src & 0x007F) as u16;
        let direction = SourceDirection::from((src & 0x0080) != 0);
        let polarity = SourcePolarity::from((src & 0x0100) != 0);
        let typ = SourceType::from((src & 0x0200) != 0);
        let ctrl = ((src >> 10) & 0x003F) as u8;

        Self {
            ctrl: ControllerPalette::from(GeneralPalette::from(ctrl)),
            direction,
            polarity,
            typ,
            index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModulatorTransform {
    Linear,
    AbsoluteValue,
    Unknown(u16),
}

impl TryFrom<u16> for ModulatorTransform {
    type Error = Error;
    fn try_from(id: u16) -> Result<Self, Self::Error> {
        match id {
            0 => Ok(Self::Linear),
            2 => Ok(Self::AbsoluteValue),
            v => Err(Error::UnknownModulatorTransform(v)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Modulator {
    pub source: ModulatorSource,
    pub destination: u16,
    pub amount: i16,
    pub amt_source: ModulatorSource,
    pub transform: ModulatorTransform,
}

impl Modulator {
    pub(crate) fn read(reader: &mut Reader) -> Result<Self, Error> {
        // Modulator record: 10 bytes
        // 0-1: src (u16) - Modulator Source
        // 2-3: dest (u16) - Generator index
        // 4-5: amount (i16)
        // 6-7: amt_src (u16) - Secondary modulator source
        // 8-9: transform (u16) - Transform type

        let src: u16 = reader.read_u16()?;
        let dest: u16 = reader.read_u16()?;
        let amount: i16 = reader.read_i16()?;
        let amt_src: u16 = reader.read_u16()?;
        let transform: u16 = reader.read_u16()?;

        // transform은 spec에서 0(Linear) 또는 2(AbsoluteValue). 0이 아니면 Linear로 fallback
        let transform = match transform {
            0 => ModulatorTransform::Linear,
            2 => ModulatorTransform::AbsoluteValue,
            _ => ModulatorTransform::Linear,
        };

        Ok(Self {
            source: ModulatorSource::from(src),
            destination: dest,
            amount,
            amt_source: ModulatorSource::from(amt_src),
            transform,
        })
    }

    pub(crate) fn read_all(
        pmod: &Chunk,
        file: &mut ScratchReader<impl Read + Seek>,
    ) -> Result<Vec<Self>, Error> {
        assert!(pmod.id() == ChunkId::pmod || pmod.id() == ChunkId::imod);
        let size = pmod.len();
        if size == 0 {
            // 빈 modulator 리스트
            Ok(Vec::new())
        } else if size % 10 != 0 {
            // spec: modulator는 10바이트 record
            Err(Error::InvalidModulatorChunkSize(size))
        } else {
            let amount = size / 10;
            let data = pmod.read_contents(file)?;
            let mut reader = Reader::new(data);
            (0..amount).map(|_| Self::read(&mut reader)).collect()
        }
    }
}

/// 기본 모듈레이터 리스트 (SF2 spec)
pub fn default_modulators() -> Vec<Modulator> {
    Vec::new()
}
