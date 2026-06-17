// raw/info.rs

use super::utils::Reader;
use crate::sf2_oxi::error::{Error, MissingChunk};
use crate::sf2_oxi::riff::{Chunk, ChunkId, ScratchReader};
use std::io::{Read, Seek};

#[derive(Debug)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    fn from_bytes(bytes: [u8; 32]) -> Self {
        Version {
            major: u16::from_le_bytes([bytes[0], bytes[1]]),
            minor: u16::from_le_bytes([bytes[2], bytes[3]]),
        }
    }
}

#[derive(Debug)]
pub struct Info {
    pub version: Version,
    pub sound_engine: String,
    pub bank_name: String,
    pub rom_name: Option<String>,
    pub rom_version: Option<Version>,
    pub creation_date: Option<String>,
    pub engineers: Option<String>,
    pub product: Option<String>,
    pub copyright: Option<String>,
    pub comments: Option<String>,
    pub software: Option<String>,
}

impl Info {
    pub(crate) fn read(
        info: &Chunk,
        file: &mut ScratchReader<impl Read + Seek>,
    ) -> Result<Self, Error> {
        assert_eq!(info.id(), ChunkId::LIST);
        assert_eq!(info.read_type(file)?, ChunkId::INFO);
        let mut version = None;
        let mut sound_engine = None;
        let mut bank_name = None;
        let mut rom_name = None;
        let mut rom_version = None;
        let mut creation_date = None;
        let mut engineers = None;
        let mut product = None;
        let mut copyright = None;
        let mut comments = None;
        let mut software = None;
        let mut iter = info.iter();
        while let Some(ch) = iter.next(file) {
            let ch = ch?;
            let id = ch.id();
            match id {
                ChunkId::ifil => {
                    let mut bytes = [0u8; 32];
                    ch.read_to(file, &mut bytes)?;
                    version = Some(Version::from_bytes(bytes));
                }
                ChunkId::isng => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    sound_engine = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::INAM => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    bank_name = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::irom => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    rom_name = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::iver => {
                    let mut bytes = [0u8; 32];
                    ch.read_to(file, &mut bytes)?;
                    rom_version = Some(Version::from_bytes(bytes));
                }
                ChunkId::ICRD => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    creation_date = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::IENG => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    engineers = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::IPRD => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    product = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::ICOP => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    copyright = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::ICMT => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    comments = Some(data.read_string(ch.len() as usize)?);
                }
                ChunkId::ISFT => {
                    let data = ch.read_contents(file)?;
                    let mut data = Reader::new(data);
                    software = Some(data.read_string(ch.len() as usize)?);
                }
                _ => return Err(Error::UnexpectedMemberOfInfo(ch)),
            }
        }
        Ok(Info {
            version: version.ok_or(MissingChunk::Version)?,
            sound_engine: sound_engine.unwrap_or_default(),
            bank_name: bank_name.unwrap_or_default(),
            rom_name,
            rom_version,
            creation_date,
            engineers,
            product,
            copyright,
            comments,
            software,
        })
    }
}
