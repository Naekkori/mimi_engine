// raw/sample_data.rs

use crate::sf2_oxi::riff::{Chunk, ChunkId};
use crate::sf2_oxi::error::Error;
use std::io::{Read, Seek};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct SampleChunk {
    pub offset: u64,
    pub len: u32,
}

impl SampleChunk {
    fn new(chunk: Chunk) -> Self {
        Self {
            offset: chunk.content_offset(),
            len: chunk.len(),
        }
    }
}

#[derive(Debug)]
pub struct SampleData {
    pub smpl: Option<SampleChunk>,
    pub sm24: Option<SampleChunk>,
}

impl SampleData {
    pub(crate) fn read<F: Read + Seek>(sdta: &Chunk, file: &mut F) -> Result<Self, Error> {
        assert_eq!(sdta.id(), ChunkId::LIST);
        assert_eq!(sdta.read_type(file)?, ChunkId::sdta);
        let mut smpl = None;
        let mut sm24 = None;
        let mut iter = sdta.iter();
        while let Some(ch) = iter.next(file) {
            let ch = ch?;
            let id = ch.id();
            match id {
                ChunkId::smpl => {
                    smpl = Some(SampleChunk::new(ch));
                }
                ChunkId::sm24 => {
                    sm24 = Some(SampleChunk::new(ch));
                }
                _ => return Err(Error::UnexpectedMemberOfSampleData(ch)),
            }
        }
        Ok(Self { smpl, sm24 })
    }
}
