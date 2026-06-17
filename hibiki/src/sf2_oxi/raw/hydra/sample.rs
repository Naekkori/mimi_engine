// raw/hydra/sample.rs

use super::utils::Reader;
use crate::sf2_oxi::riff::{Chunk, ChunkId, ScratchReader};
use crate::sf2_oxi::error::Error;
use std::io::{Read, Seek};

#[derive(Debug, Clone)]
pub struct SampleHeader {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub loop_start: u32,
    pub loop_end: u32,
    pub sample_rate: u32,
    pub origpitch: u8,
    pub pitchadj: i8,
    pub sample_link: u16,
    pub sample_type: SampleLink,
}

impl SampleHeader {
    pub(crate) fn read(reader: &mut Reader) -> Result<Self, Error> {
        let name: String = reader.read_string(20)?.trim_end().to_owned();
        let start: u32 = reader.read_u32()?;
        let end: u32 = reader.read_u32()?;
        let loop_start: u32 = reader.read_u32()?;
        let loop_end: u32 = reader.read_u32()?;
        let sample_rate: u32 = reader.read_u32()?;
        let origpitch: u8 = reader.read_u8()?;
        let pitchadj: i8 = reader.read_i8()?;
        let sample_link: u16 = reader.read_u16()?;
        let sample_type: u16 = reader.read_u16()?;
        let sample_type = match sample_type {
            0 => SampleLink::None,
            1 => SampleLink::MonoSample,
            2 => SampleLink::RightSample,
            4 => SampleLink::LeftSample,
            8 => SampleLink::LinkedSample,
            0x8001 => SampleLink::RomMonoSample,
            0x8002 => SampleLink::RomRightSample,
            0x8004 => SampleLink::RomLeftSample,
            0x8008 => SampleLink::RomLinkedSample,
            0x11 => SampleLink::VorbisMonoSample,
            0x12 => SampleLink::VorbisRightSample,
            0x14 => SampleLink::VorbisLeftSample,
            0x18 => SampleLink::VorbisLinkedSample,
            v => return Err(Error::UnknownSampleType(v)),
        };
        Ok(Self { name, start, end, loop_start, loop_end, sample_rate, origpitch, pitchadj, sample_link, sample_type })
    }

    pub(crate) fn read_all(
        phdr: &Chunk,
        file: &mut ScratchReader<impl Read + Seek>,
    ) -> Result<Vec<Self>, Error> {
        assert_eq!(phdr.id(), ChunkId::shdr);
        let size = phdr.len();
        if size % 46 != 0 || size == 0 {
            Err(Error::InvalidSampleChunkSize(size))
        } else {
            let amount = size / 46;
            let data = phdr.read_contents(file)?;
            let mut reader = Reader::new(data);
            (0..amount).map(|_| Self::read(&mut reader)).collect()
        }
    }
}

#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub enum SampleLink {
    None = 0,
    MonoSample = 0x1,
    RightSample = 0x2,
    LeftSample = 0x4,
    LinkedSample = 0x8,
    RomMonoSample = 0x8001,
    RomRightSample = 0x8002,
    RomLeftSample = 0x8004,
    RomLinkedSample = 0x8008,
    VorbisMonoSample = 0x11,
    VorbisRightSample = 0x12,
    VorbisLeftSample = 0x14,
    VorbisLinkedSample = 0x18,
}
