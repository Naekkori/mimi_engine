// debug_sf2.rs - SF2 파서 디버깅

use hibiki::sf2::SoundFont2;
use std::io::{Read, Seek, SeekFrom};

fn main() {
    let path = "d:/source/mimi_engine/assets/soundfont.SF2";
    let mut file = std::fs::File::open(path).expect("Failed to open SF2");

    let mut header = [0u8; 12];
    let _ = file.read_exact(&mut header);
    println!("HEX: {:?}", &header[..12]);
    let riff_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    println!("RIFF size: {} bytes ({} MB)", riff_size, riff_size as f64 / 1024.0 / 1024.0);

    file.seek(SeekFrom::Start(0)).unwrap();

    match SoundFont2::load(&mut file) {
        Ok(sf) => {
            println!("\n파싱 성공!");
            println!("info: bank_name='{}', engine='{}'", sf.info.bank_name, sf.info.sound_engine);
            println!("version: {}.{}", sf.info.version.major, sf.info.version.minor);
            println!("샘플 수: {}", sf.sample_headers.len());
            println!("악기 수: {}", sf.instruments.len());
            println!("프리셋 수: {}", sf.presets.len());
        }
        Err(e) => {
            eprintln!("파싱 실패: {:?}", e);
        }
    }
}
