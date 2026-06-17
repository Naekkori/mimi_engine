// debug_sf2.rs - SF2 파서 디버깅

use hibiki::sf2::Sf2Parser;
use std::io::{Read, Seek, SeekFrom};

fn main() {
    let path = "d:/source/mimi_engine/assets/soundfont.SF2";
    let mut file = std::fs::File::open(path).expect("Failed to open SF2");

    // 처음 100바이트 읽기
    let mut header = [0u8; 100];
    let _ = file.read_exact(&mut header);

    println!("=== SF2 첫 100바이트 ===");
    print!("HEX: ");
    for b in &header[..12] {
        print!("{:02X} ", b);
    }
    println!();
    print!("ASCII: ");
    for b in &header[..12] {
        if b.is_ascii_graphic() || *b == b' ' {
            print!("{}", *b as char);
        } else {
            print!(".");
        }
    }
    println!();

    // RIFF 크기
    let riff_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    println!("RIFF size: {} bytes ({} MB)", riff_size, riff_size as f64 / 1024.0 / 1024.0);

    // 파일 처음으로 되돌리고 파싱
    file.seek(SeekFrom::Start(0)).unwrap();

    println!("\n=== 파싱 시작 ===");
    match Sf2Parser::parse(&mut file) {
        Ok(sf) => {
            println!("파싱 성공!");
            println!("샘플 수: {}", sf.samples.len());
            println!("악기 수: {}", sf.instruments.len());
            println!("프리셋 수: {}", sf.presets.len());
            if let Some(s) = sf.samples.first() {
                println!("첫 샘플: name='{}', start={}, end={}, sample_rate={}",
                    s.name, s.start, s.end, s.sample_rate);
            }
            println!("smpl_data len: {}", sf.smpl_data.len());
            // First samples
            let n = sf.smpl_data.len().min(20);
            print!("First {} samples: ", n);
            for i in 0..n {
                print!("{} ", sf.smpl_data[i]);
            }
            println!();
            // First sample's data
            if let Some(s) = sf.samples.first() {
                let start = s.start as usize;
                let end = (s.start as usize + 20).min(s.end as usize).min(sf.smpl_data.len());
                print!("Sample '{}' (start={} end={}) first 20: ", s.name, s.start, s.end);
                for i in start..end {
                    print!("{} ", sf.smpl_data[i]);
                }
                println!();
            }
            if let Some(p) = sf.presets.iter().find(|p| p.preset_num == 0 && p.bank == 0) {
                println!("프리셋 0/0: name='{}', bank={}, zone수={}", p.name, p.bank, p.zones.len());
                for z in &p.zones {
                    println!("  zone: key={:?}, vel={:?}, inst_idx={:?}", z.key_range, z.velocity_range, z.instrument_index);
                }
            }
            // bank=0 program=0과 가까운 preset들
            for p in sf.presets.iter().take(15) {
                println!("  preset: name='{}', bank={}, prog={}, zone수={}", p.name, p.bank, p.preset_num, p.zones.len());
            }
            if let Some(inst) = sf.instruments.first() {
                println!("악기 0: name='{}', zone수={}", inst.name, inst.zones.len());
                for z in &inst.zones {
                    println!("  zone: key={:?}, vel={:?}, sample_idx={:?}", z.key_range, z.velocity_range, z.sample_index);
                }
            }
        }
        Err(e) => {
            eprintln!("파싱 실패: {:?}", e);
        }
    }
}
