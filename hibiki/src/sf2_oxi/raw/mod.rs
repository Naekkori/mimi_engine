// raw/mod.rs

pub(crate) mod utils;
pub mod hydra;
mod info;
mod sample_data;

use crate::sf2_oxi::error::MissingChunk;
use crate::sf2_oxi::riff::{Chunk, ScratchReader};
use crate::sf2_oxi::{error::Error, riff::ChunkId};
use std::io::{Read, Seek};

pub use hydra::*;
pub use info::*;
pub use sample_data::{SampleData, SampleChunk};

#[derive(Debug)]
pub struct RawSoundFontData {
    pub info: Info,
    pub sample_data: SampleData,
    pub hydra: Hydra,
}

impl RawSoundFontData {
    pub fn load<F: Read + Seek>(file: &mut F) -> Result<Self, Error> {
        let sfbk = Chunk::read(file, 0)?;
        assert_eq!(sfbk.id(), ChunkId::RIFF);
        assert_eq!(sfbk.read_type(file)?, ChunkId::sfbk);
        let mut info = None;
        let mut sample_data = None;
        let mut hydra = None;
        let mut file = ScratchReader::new(file);
        let mut iter = sfbk.iter();
        while let Some(ch) = iter.next(&mut file) {
            let ch = ch?;
            assert_eq!(ch.id(), ChunkId::LIST);
            match ch.read_type(&mut file)? {
                ChunkId::INFO => {
                    info = Some(Info::read(&ch, &mut file)?);
                }
                ChunkId::sdta => {
                    sample_data = Some(SampleData::read(&ch, &mut file)?);
                }
                ChunkId::pdta => {
                    hydra = Some(Hydra::read(&ch, &mut file)?);
                }
                _ => return Err(Error::UnexpectedMemberOfRoot(ch)),
            }
        }
        Ok(RawSoundFontData {
            info: info.ok_or(MissingChunk::Info)?,
            sample_data: sample_data.ok_or(MissingChunk::SampleData)?,
            hydra: hydra.ok_or(MissingChunk::Hydra)?,
        })
    }
}
