// raw/hydra/mod.rs

use super::utils;

pub mod generator;
pub mod modulator;
pub mod bag;
pub mod preset;
pub mod instrument;
pub mod sample;

pub use generator::{Generator, GeneratorAmount, GeneratorAmountRange, GeneratorType};
pub use modulator::{
    default_modulators, ControllerPalette, GeneralPalette, Modulator, ModulatorSource,
    ModulatorTransform, SourceDirection, SourcePolarity, SourceType,
};
pub use bag::Bag;
pub use preset::PresetHeader;
pub use instrument::InstrumentHeader;
pub use sample::{SampleHeader, SampleLink};

use crate::sf2_oxi::error::MissingChunk;
use crate::sf2_oxi::riff::{Chunk, ScratchReader};
use crate::sf2_oxi::{error::Error, riff::ChunkId};
use std::io::{Read, Seek};

#[derive(Debug)]
pub struct Hydra {
    pub preset_headers: Vec<PresetHeader>,
    pub preset_bags: Vec<Bag>,
    pub preset_modulators: Vec<Modulator>,
    pub preset_generators: Vec<Generator>,
    pub instrument_headers: Vec<InstrumentHeader>,
    pub instrument_bags: Vec<Bag>,
    pub instrument_modulators: Vec<Modulator>,
    pub instrument_generators: Vec<Generator>,
    pub sample_headers: Vec<SampleHeader>,
}

impl Hydra {
    pub(crate) fn read(
        pdta: &Chunk,
        file: &mut ScratchReader<impl Read + Seek>,
    ) -> Result<Self, Error> {
        assert_eq!(pdta.id(), ChunkId::LIST);
        assert_eq!(pdta.read_type(file)?, ChunkId::pdta);
        let mut preset_headers = None;
        let mut preset_bags = None;
        let mut preset_modulators = None;
        let mut preset_generators = None;
        let mut instrument_headers = None;
        let mut instrument_bags = None;
        let mut instrument_modulators = None;
        let mut instrument_generators = None;
        let mut sample_headers = None;
        let mut iter = pdta.iter();
        while let Some(ch) = iter.next(file) {
            let ch = ch?;
            match ch.id() {
                ChunkId::phdr => preset_headers = Some(PresetHeader::read_all(&ch, file)?),
                ChunkId::pbag => preset_bags = Some(Bag::read_all(&ch, file)?),
                ChunkId::pmod => preset_modulators = Some(Modulator::read_all(&ch, file)?),
                ChunkId::pgen => preset_generators = Some(Generator::read_all(&ch, file)?),
                ChunkId::inst => instrument_headers = Some(InstrumentHeader::read_all(&ch, file)?),
                ChunkId::ibag => instrument_bags = Some(Bag::read_all(&ch, file)?),
                ChunkId::imod => instrument_modulators = Some(Modulator::read_all(&ch, file)?),
                ChunkId::igen => instrument_generators = Some(Generator::read_all(&ch, file)?),
                ChunkId::shdr => sample_headers = Some(SampleHeader::read_all(&ch, file)?),
                _ => return Err(Error::UnexpectedMemberOfHydra(ch)),
            }
        }
        use MissingChunk::*;
        Ok(Self {
            preset_headers: preset_headers.ok_or(PresetHeaders)?,
            preset_bags: preset_bags.ok_or(PresetBags)?,
            preset_modulators: preset_modulators.ok_or(PresetModulators)?,
            preset_generators: preset_generators.ok_or(PresetGenerators)?,
            instrument_headers: instrument_headers.ok_or(InstrumentHeaders)?,
            instrument_bags: instrument_bags.ok_or(InstrumentBags)?,
            instrument_modulators: instrument_modulators.ok_or(InstrumentModulators)?,
            instrument_generators: instrument_generators.ok_or(InstrumentGenerators)?,
            sample_headers: sample_headers.ok_or(SampleHeaders)?,
        })
    }
}
