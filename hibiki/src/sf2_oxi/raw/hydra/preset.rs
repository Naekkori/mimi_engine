// raw/hydra/preset.rs

use super::utils::Reader;
use crate::sf2_oxi::riff::{Chunk, ChunkId, ScratchReader};
use crate::sf2_oxi::error::Error;
use std::io::{Read, Seek};

#[derive(Debug, Clone)]
pub struct PresetHeader {
    pub name: String,
    pub preset: u16,
    pub bank: u16,
    pub bag_id: u16,
    pub library: u32,
    pub genre: u32,
    pub morphology: u32,
}

impl PresetHeader {
    pub(crate) fn read(reader: &mut Reader) -> Result<Self, Error> {
        let name: String = reader.read_string(20)?.trim_end().to_owned();
        let preset: u16 = reader.read_u16()?;
        let bank: u16 = reader.read_u16()?;
        let bag_id: u16 = reader.read_u16()?;
        let library: u32 = reader.read_u32()?;
        let genre: u32 = reader.read_u32()?;
        let morphology: u32 = reader.read_u32()?;
        Ok(Self { name, preset, bank, bag_id, library, genre, morphology })
    }

    pub(crate) fn read_all(
        phdr: &Chunk,
        file: &mut ScratchReader<impl Read + Seek>,
    ) -> Result<Vec<Self>, Error> {
        assert_eq!(phdr.id(), ChunkId::phdr);
        let size = phdr.len();
        if size % 38 != 0 || size == 0 {
            Err(Error::InvalidPresetChunkSize(size))
        } else {
            let amount = size / 38;
            let data = phdr.read_contents(file)?;
            let mut reader = Reader::new(data);
            (0..amount).map(|_| Self::read(&mut reader)).collect()
        }
    }
}
