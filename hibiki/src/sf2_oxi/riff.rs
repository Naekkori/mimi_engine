// OxiSynth의 soundfont-rs riff.rs
// 원본: https://github.com/PolyMeilex/OxiSynth/blob/master/soundfont-rs/src/riff.rs

use std::{
    fmt,
    io::{Read, Seek, SeekFrom},
};

pub struct ScratchReader<T> {
    pub buff: Vec<u8>,
    pub io: T,
}

impl<T> ScratchReader<T> {
    pub fn new(io: T) -> Self {
        Self {
            buff: Vec::new(),
            io,
        }
    }
}

impl<T: Read> Read for ScratchReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.io.read(buf)
    }
}

impl<T: Seek> Seek for ScratchReader<T> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.io.seek(pos)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub struct ChunkId(pub [u8; 4]);

macro_rules! def_ids {
    (
        $(
            $(#[doc = $doc:expr])?
            $ident: ident
        ),*
        $(,)?
    ) => {
        $(
            $(#[doc = $doc])?
            #[allow(non_upper_case_globals)]
            pub const $ident: Self = Self({
                let v = stringify!($ident).as_bytes();
                [v[0], v[1], v[2], v[3]]
            });
        )*
    };
}

impl ChunkId {
    pub const RIFF: Self = Self(*b"RIFF");
    pub const LIST: Self = Self(*b"LIST");
    pub const sfbk: Self = Self(*b"sfbk");
    pub const INFO: Self = Self(*b"INFO");
    pub const sdta: Self = Self(*b"sdta");
    pub const pdta: Self = Self(*b"pdta");
    pub const ifil: Self = Self(*b"ifil");
    pub const isng: Self = Self(*b"isng");
    pub const INAM: Self = Self(*b"INAM");
    pub const irom: Self = Self(*b"irom");
    pub const iver: Self = Self(*b"iver");
    pub const ICRD: Self = Self(*b"ICRD");
    pub const IENG: Self = Self(*b"IENG");
    pub const IPRD: Self = Self(*b"IPRD");
    pub const ICOP: Self = Self(*b"ICOP");
    pub const ICMT: Self = Self(*b"ICMT");
    pub const ISFT: Self = Self(*b"ISFT");
    pub const smpl: Self = Self(*b"smpl");
    pub const sm24: Self = Self(*b"sm24");
    pub const phdr: Self = Self(*b"phdr");
    pub const pbag: Self = Self(*b"pbag");
    pub const pmod: Self = Self(*b"pmod");
    pub const pgen: Self = Self(*b"pgen");
    pub const inst: Self = Self(*b"inst");
    pub const ibag: Self = Self(*b"ibag");
    pub const imod: Self = Self(*b"imod");
    pub const igen: Self = Self(*b"igen");
    pub const shdr: Self = Self(*b"shdr");
}

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(v) = std::str::from_utf8(&self.0) {
            write!(f, "{v:?}")
        } else {
            write!(f, "{:?}", self.0)
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct Chunk {
    pub pos: u64,
    pub id: ChunkId,
    pub len: u32,
}

pub struct Iter {
    end: u64,
    cur: u64,
}

impl Iter {
    pub fn next<T: Seek + Read>(&mut self, stream: &mut T) -> Option<std::io::Result<Chunk>> {
        if self.cur >= self.end {
            return None;
        }
        let chunk = match Chunk::read(stream, self.cur) {
            Ok(chunk) => chunk,
            Err(err) => return Some(Err(err)),
        };
        let len = chunk.len() as u64;
        self.cur = self.cur + len + 8 + (len % 2);
        Some(Ok(chunk))
    }
}

impl Chunk {
    pub fn id(&self) -> ChunkId {
        self.id
    }
    pub fn len(&self) -> u32 {
        self.len
    }
    pub fn content_offset(&self) -> u64 {
        self.pos + 8
    }
    pub fn read_type<T>(&self, stream: &mut T) -> std::io::Result<ChunkId>
    where
        T: Read + Seek,
    {
        stream.seek(SeekFrom::Start(self.pos + 8))?;
        let mut fourcc: [u8; 4] = [0; 4];
        stream.read_exact(&mut fourcc)?;
        Ok(ChunkId(fourcc))
    }
    pub fn read<T>(stream: &mut T, pos: u64) -> std::io::Result<Chunk>
    where
        T: Read + Seek,
    {
        stream.seek(SeekFrom::Start(pos))?;
        let mut fourcc: [u8; 4] = [0; 4];
        stream.read_exact(&mut fourcc)?;
        let mut len: [u8; 4] = [0; 4];
        stream.read_exact(&mut len)?;
        Ok(Chunk {
            pos,
            id: ChunkId(fourcc),
            len: u32::from_le_bytes(len),
        })
    }
    pub fn read_to<T>(&self, stream: &mut T, buf: &mut [u8]) -> std::io::Result<()>
    where
        T: Read + Seek,
    {
        stream.seek(SeekFrom::Start(self.content_offset()))?;
        stream.read_exact(buf)?;
        Ok(())
    }
    pub fn read_contents<'a, T>(
        &self,
        stream: &'a mut ScratchReader<T>,
    ) -> std::io::Result<&'a [u8]>
    where
        T: Read + Seek,
    {
        let ScratchReader { buff, io } = stream;
        io.seek(SeekFrom::Start(self.content_offset()))?;
        buff.resize(self.len as usize, 0);
        io.read_exact(buff)?;
        Ok(buff)
    }
    pub fn iter(&self) -> Iter {
        Iter {
            cur: self.pos + 12,
            end: self.pos + 4 + (self.len as u64),
        }
    }
}
