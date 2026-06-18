use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

#[derive(Default, Debug, Clone)]
pub struct SoundFontInfo {
    pub version: Option<String>,  // IFIL
    pub author: Option<String>,   // IENG
    pub target: Option<String>,   // INAM
    pub comments: Option<String>, // ICMT
    pub created: Option<String>,  // ICRD
    pub tool: Option<String>,     // ISFT
    pub copyright: Option<String>,// ICMV
}
pub fn read_sf2_info(path: &str) -> std::io::Result<SoundFontInfo> {
    let mut f = File::open(path)?;
    let mut header = [0u8; 12];
    f.read_exact(&mut header)?;
    // header = b"RIFF" + size + b"sfbk"

    let mut info = SoundFontInfo::default();
    parse_list(&mut f, &mut info)?;
    Ok(info)
}

fn parse_list(f: &mut File, info: &mut SoundFontInfo) -> std::io::Result<()> {
    let id = read_tag(f)?;
    let size = read_u32_le(f)? as u64;
    let end = f.seek(SeekFrom::Current(0))? + size;

    if &id == b"LIST" {
        let list_type = read_tag(f)?;
        if &list_type == b"INFO" {
            while f.seek(SeekFrom::Current(0))? < end {
                let sub_id = read_tag(f)?;
                let sub_size = read_u32_le(f)? as usize;
                let mut buf = vec![0u8; sub_size];
                f.read_exact(&mut buf)?;
                let value = String::from_utf8_lossy(&buf).trim_end_matches('\0').to_string();

                match &sub_id {
                    b"IFIL" => info.version = Some(value),
                    b"IENG" => info.author = Some(value),
                    b"INAM" => info.target = Some(value),
                    b"ICMT" => info.comments = Some(value),
                    b"ICRD" => info.created = Some(value),
                    b"ISFT" => info.tool = Some(value),
                    b"ICOP" | b"ICMV" => info.copyright = Some(value),
                    _ => {}
                }

                // RIFF 청크는 2바이트 정렬
                if sub_size % 2 == 1 {
                    f.seek(SeekFrom::Current(1))?;
                }
            }
        }
    }
    Ok(())
}

fn read_tag(f: &mut File) -> std::io::Result<[u8; 4]> {
    let mut tag = [0u8; 4];
    f.read_exact(&mut tag)?;
    Ok(tag)
}

fn read_u32_le(f: &mut File) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}