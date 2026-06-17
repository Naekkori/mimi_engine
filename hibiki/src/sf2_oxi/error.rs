// OxiSynth의 soundfont-rs 코드를 hibiki에 vendoring
// 원본: https://github.com/PolyMeilex/OxiSynth/blob/master/soundfont-rs/src/error.rs
// 라이센스: MIT

use std::array::TryFromSliceError;
use std::io;
use std::str::Utf8Error;

use crate::sf2_oxi::riff::Chunk;

#[allow(unused)]
type Result<T> = std::result::Result<T, self::Error>;

#[derive(Debug)]
pub enum Error {
    StringError(Utf8Error),
    Io(io::Error),
    NumSliceError(TryFromSliceError),
    InvalidBagChunkSize(u32),
    InvalidGeneratorChunkSize(u32),
    InvalidInstrumentChunkSize(u32),
    InvalidModulatorChunkSize(u32),
    InvalidPresetChunkSize(u32),
    InvalidSampleChunkSize(u32),
    UnknownGeneratorType(u16),
    UnknownSampleType(u16),
    UnknownModulatorTransform(u16),
    UnexpectedMemberOfRoot(Chunk),
    UnexpectedMemberOfHydra(Chunk),
    UnexpectedMemberOfInfo(Chunk),
    UnexpectedMemberOfSampleData(Chunk),
    MissingChunk(MissingChunk),
}

#[derive(Debug)]
pub enum MissingChunk {
    Info,
    SampleData,
    Hydra,
    Version,
    PresetHeaders,
    PresetBags,
    PresetModulators,
    PresetGenerators,
    InstrumentHeaders,
    InstrumentBags,
    InstrumentModulators,
    InstrumentGenerators,
    SampleHeaders,
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl From<MissingChunk> for Error {
    fn from(err: MissingChunk) -> Self {
        Self::MissingChunk(err)
    }
}

impl From<Utf8Error> for Error {
    fn from(err: Utf8Error) -> Self {
        Self::StringError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<TryFromSliceError> for Error {
    fn from(err: TryFromSliceError) -> Self {
        Self::NumSliceError(err)
    }
}
