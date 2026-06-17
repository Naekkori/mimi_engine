// raw/hydra/generator.rs

use super::utils::Reader;
use crate::sf2_oxi::error::Error;
use crate::sf2_oxi::riff::{Chunk, ChunkId, ScratchReader};
use crate::sf2_oxi::SfEnum;
use std::io::{Read, Seek};

#[derive(Debug, Clone)]
pub enum GeneratorAmount {
    I16(i16),
    U16(u16),
    Range(GeneratorAmountRange),
}

impl GeneratorAmount {
    pub fn as_i16(&self) -> Option<&i16> {
        if let Self::I16(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_u16(&self) -> Option<&u16> {
        if let Self::U16(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_range(&self) -> Option<&GeneratorAmountRange> {
        if let Self::Range(r) = self {
            Some(r)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GeneratorAmountRange {
    pub low: u8,
    pub high: u8,
}

#[derive(Debug, Clone)]
pub struct Generator {
    pub ty: SfEnum<GeneratorType, u16>,
    pub amount: GeneratorAmount,
}

impl Generator {
    pub(crate) fn read(reader: &mut Reader) -> Result<Self, Error> {
        let id: u16 = reader.read_u16()?;
        let ty = GeneratorType::try_from(id)
            .map(SfEnum::Value)
            .unwrap_or(SfEnum::Unknown(id));
        let amount = match ty.into_result().unwrap_or(GeneratorType::EndOper) {
            GeneratorType::KeyRange | GeneratorType::VelRange => {
                GeneratorAmount::Range(GeneratorAmountRange {
                    low: reader.read_u8()?,
                    high: reader.read_u8()?,
                })
            }
            GeneratorType::Instrument | GeneratorType::SampleID => {
                GeneratorAmount::U16(reader.read_u16()?)
            }
            _ => GeneratorAmount::I16(reader.read_i16()?),
        };
        Ok(Self { ty, amount })
    }

    pub(crate) fn read_all(
        pmod: &Chunk,
        file: &mut ScratchReader<impl Read + Seek>,
    ) -> Result<Vec<Self>, Error> {
        assert!(pmod.id() == ChunkId::pgen || pmod.id() == ChunkId::igen);
        let size = pmod.len();
        if size % 4 != 0 || size == 0 {
            Err(Error::InvalidGeneratorChunkSize(size))
        } else {
            let amount = size / 4;
            let data = pmod.read_contents(file)?;
            let mut reader = Reader::new(data);
            (0..amount).map(|_| Self::read(&mut reader)).collect()
        }
    }
}

impl SfEnum<GeneratorType, u16> {
    pub fn as_raw(&self) -> u16 {
        match *self {
            Self::Value(v) => v as u16,
            Self::Unknown(v) => v,
        }
    }
    pub fn into_result(&self) -> Result<GeneratorType, Error> {
        match *self {
            Self::Value(v) => Ok(v),
            Self::Unknown(v) => Err(Error::UnknownGeneratorType(v)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[repr(u16)]
pub enum GeneratorType {
    StartAddrsOffset = 0,
    EndAddrsOffset = 1,
    StartloopAddrsOffset = 2,
    EndloopAddrsOffset = 3,
    StartAddrsCoarseOffset = 4,
    ModLfoToPitch = 5,
    VibLfoToPitch = 6,
    ModEnvToPitch = 7,
    InitialFilterFc = 8,
    InitialFilterQ = 9,
    ModLfoToFilterFc = 10,
    ModEnvToFilterFc = 11,
    EndAddrsCoarseOffset = 12,
    ModLfoToVolume = 13,
    Unused1 = 14,
    ChorusEffectsSend = 15,
    ReverbEffectsSend = 16,
    Pan = 17,
    Unused2 = 18,
    Unused3 = 19,
    Unused4 = 20,
    DelayModLFO = 21,
    FreqModLFO = 22,
    DelayVibLFO = 23,
    FreqVibLFO = 24,
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
    KeyRange = 43,
    VelRange = 44,
    StartloopAddrsCoarseOffset = 45,
    Keynum = 46,
    Velocity = 47,
    InitialAttenuation = 48,
    Reserved2 = 49,
    EndloopAddrsCoarseOffset = 50,
    CoarseTune = 51,
    FineTune = 52,
    SampleID = 53,
    SampleModes = 54,
    Reserved3 = 55,
    ScaleTuning = 56,
    ExclusiveClass = 57,
    OverridingRootKey = 58,
    Unused5 = 59,
    EndOper = 60,
}

impl TryFrom<u16> for GeneratorType {
    type Error = Error;
    fn try_from(id: u16) -> Result<Self, Self::Error> {
        if id <= 60 {
            Ok(unsafe { std::mem::transmute::<u16, Self>(id) })
        } else {
            Err(Error::UnknownGeneratorType(id))
        }
    }
}
