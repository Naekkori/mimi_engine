// sf2/parser.rs - SF2 파일 파서

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};

use super::types::*;

/// 파싱 오류
#[derive(Debug)]
pub enum ParseError {
    IoError(std::io::Error),
    InvalidFormat(String),
    InvalidChunk(String),
    UnexpectedEof,
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::IoError(e)
    }
}

/// 4CC를 문자열로 변환
fn four_cc_to_string(id: &[u8; 4]) -> String {
    String::from_utf8_lossy(id).to_string()
}

/// SF2 파서
pub struct Sf2Parser;

impl Sf2Parser {
    /// SF2 파일 파싱
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Sf2File, ParseError> {
        // RIFF 헤더 읽기
        let riff_id = Self::read_four_cc(reader)?;
        if &riff_id != b"RIFF" {
            return Err(ParseError::InvalidFormat(format!(
                "Expected RIFF, got: {}",
                four_cc_to_string(&riff_id)
            )));
        }

        let _file_size = reader.read_u32::<LittleEndian>()?;
        let form_type = Self::read_four_cc(reader)?;
        if &form_type != b"sfbk" {
            return Err(ParseError::InvalidFormat(format!(
                "Expected sfbk, got: {}",
                four_cc_to_string(&form_type)
            )));
        }

        // INFO, sdta, pdta 청크 파싱
        let mut header = None;
        let mut samples = Vec::new();
        let mut instruments: Vec<Instrument> = Vec::new();
        let mut presets: Vec<Preset> = Vec::new();

        // 임시 데이터 (나중에 처리)
        let mut phdr_data: Vec<u8> = Vec::new();
        let mut pbag_data: Vec<u8> = Vec::new();
        let mut pgen_data: Vec<u8> = Vec::new();
        let mut ihdr_data: Vec<u8> = Vec::new();
        let mut ibag_data: Vec<u8> = Vec::new();
        let mut igen_data: Vec<u8> = Vec::new();
        let mut shdr_data: Vec<u8> = Vec::new();
        let mut smpl_data_u8: Vec<u8> = Vec::new();

        let file_end = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(12))?; // RIFF 헤더(12바이트) 다음으로

        loop {
            let pos = reader.stream_position()?;
            if pos >= file_end {
                break;
            }
            if pos >= 2_147_483_648u64 {
                break;
            }

            let chunk_id = Self::read_four_cc(reader);
            if chunk_id.is_err() {
                break;
            }
            let chunk_id = chunk_id.unwrap();
            let chunk_size = reader.read_u32::<LittleEndian>()?;

            if chunk_size > 524_288_000 {
                // 청크가 너무 크면 중지 (500MB)
                break;
            }

            // 0 바이트 청크는 무한 루프 방지
            if chunk_size == 0 {
                break;
            }

            let chunk_start = reader.stream_position()?;

            match chunk_id.as_ref() {
                b"LIST" => {
                    // LIST 청크 - form-type을 읽고 해당 섹션 처리
                    let form_type = Self::read_four_cc(reader)?;
                    let list_size = chunk_size - 4; // form-type 4바이트 제외

                    match form_type.as_ref() {
                        b"INFO" => {
                            let info_start = reader.stream_position()?;
                            header = Some(Self::parse_info_chunk(reader, list_size)?);
                            // INFO LIST 끝으로 강제 이동
                            let target = info_start + list_size as u64;
                            let pad = if list_size % 2 == 1 { 1 } else { 0 };
                            reader.seek(SeekFrom::Start(target + pad))?;
                        }
                        b"sdta" => {
                            // smpl 청크 읽기
                            let mut remaining = list_size;
                            let mut iter_count = 0;
                            const MAX_ITER: usize = 100;
                            while remaining > 8 && iter_count < MAX_ITER {
                                iter_count += 1;
                                let sub_id = Self::read_four_cc(reader)?;
                                let sub_size = reader.read_u32::<LittleEndian>()?;
                                remaining -= 8;

                                if sub_size > remaining + 8 {
                                    break;
                                }

                                if sub_id.as_ref() == b"smpl" {
                                    let mut buf = vec![0u8; sub_size as usize];
                                    reader.read_exact(&mut buf)?;
                                    smpl_data_u8 = buf;
                                } else {
                                    reader.seek(SeekFrom::Current(sub_size as i64))?;
                                }
                                remaining -= sub_size;
                                if sub_size % 2 == 1 && remaining > 0 {
                                    reader.seek(SeekFrom::Current(1))?;
                                    remaining -= 1;
                                }
                            }
                            // 남은 바이트가 있으면 그만큼 이동
                            if remaining > 0 {
                                reader.seek(SeekFrom::Current(remaining as i64))?;
                            }
                        }
                        b"pdta" => {
                            let mut remaining = list_size;
                            let mut iter_count = 0;
                            const MAX_ITER: usize = 100;
                            while remaining > 8 && iter_count < MAX_ITER {
                                iter_count += 1;
                                let sub_id = Self::read_four_cc(reader)?;
                                let sub_size = reader.read_u32::<LittleEndian>()?;
                                remaining -= 8;

                                if sub_size > remaining + 8 {
                                    break;
                                }

                                let mut sub_data = vec![0u8; sub_size as usize];
                                reader.read_exact(&mut sub_data)?;

                                match sub_id.as_ref() {
                                    b"phdr" => phdr_data = sub_data,
                                    b"pbag" => pbag_data = sub_data,
                                    b"pgen" => pgen_data = sub_data,
                                    b"inst" => ihdr_data = sub_data, // SF2 spec: "inst" = instrument headers
                                    b"ibag" => ibag_data = sub_data,
                                    b"igen" => igen_data = sub_data,
                                    b"shdr" => shdr_data = sub_data,
                                    _ => {}
                                }

                                remaining -= sub_size;
                                if sub_size % 2 == 1 && remaining > 0 {
                                    reader.seek(SeekFrom::Current(1))?;
                                    remaining -= 1;
                                }
                            }
                            // 남은 바이트가 있으면 그만큼 이동
                            if remaining > 0 {
                                reader.seek(SeekFrom::Current(remaining as i64))?;
                            }
                        }
                        _ => {
                            // 알 수 없는 LIST - 건너뛰기
                            reader.seek(SeekFrom::Current(list_size as i64))?;
                        }
                    }
                    // LIST 청크 2바이트 정렬
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                    }
                }
                b"INFO" => {
                    header = Some(Self::parse_info_chunk(reader, chunk_size)?);
                }
                b"sdta" => {
                    // smpl 청크 읽기
                    let _sub_id = Self::read_four_cc(reader)?;
                    let _sub_size = reader.read_u32::<LittleEndian>()?;
                    let mut buf = vec![0u8; (chunk_size - 8) as usize];
                    reader.read_exact(&mut buf)?;
                    smpl_data_u8 = buf;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                    }
                }
                b"pdta" => {
                    let mut remaining = chunk_size;
                    while remaining > 8 {
                        let sub_id = Self::read_four_cc(reader)?;
                        let sub_size = reader.read_u32::<LittleEndian>()?;
                        remaining -= 8;

                        let mut sub_data = vec![0u8; sub_size as usize];
                        reader.read_exact(&mut sub_data)?;

                        match sub_id.as_ref() {
                            b"phdr" => phdr_data = sub_data,
                            b"pbag" => pbag_data = sub_data,
                            b"pgen" => pgen_data = sub_data,
                            b"ihdr" => ihdr_data = sub_data,
                            b"ibag" => ibag_data = sub_data,
                            b"igen" => igen_data = sub_data,
                            b"shdr" => shdr_data = sub_data,
                            _ => {}
                        }

                        remaining -= sub_size;
                        if sub_size % 2 == 1 && remaining > 0 {
                            reader.seek(SeekFrom::Current(1))?;
                            remaining -= 1;
                        }
                    }
                }
                _ => {
                    reader.seek(SeekFrom::Current(chunk_size as i64))?;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                    }
                }
            }
        }

        // 샘플 데이터 파싱

        // smpl_data를 i16으로 한 번만 변환 (Arc로 공유)
        let mut smpl_data_i16: Vec<i16> = Vec::with_capacity(smpl_data_u8.len() / 2);
        let mut cursor = std::io::Cursor::new(&smpl_data_u8);
        for _ in 0..smpl_data_u8.len() / 2 {
            smpl_data_i16.push(cursor.read_i16::<LittleEndian>().unwrap_or(0));
        }
        let smpl_arc = Arc::new(smpl_data_i16);

        samples = Self::parse_shdr_and_samples(&shdr_data, &smpl_arc)?;

        // 악기, 프리셋 파싱
        let preset_data = Self::process_preset_data(
            &phdr_data,
            &pbag_data,
            &pgen_data,
            &ihdr_data,
            &ibag_data,
            &igen_data,
            samples.len(),
        )?;
        presets = preset_data.0;
        instruments = preset_data.1;

        // 디버그: pgen 41번 통계
        let mut inst_41_count = 0;
        let mut i = 0;
        while i + 4 <= pgen_data.len() {
            let t = u16::from_le_bytes([pgen_data[i], pgen_data[i+1]]);
            if t == 41 {
                inst_41_count += 1;
            }
            i += 4;
        }

        // 디버그: igen 53번 통계
        let mut samp_53_count = 0;
        let mut i = 0;
        while i + 4 <= igen_data.len() {
            let t = u16::from_le_bytes([igen_data[i], igen_data[i+1]]);
            if t == 53 {
                samp_53_count += 1;
            }
            i += 4;
        }

        Ok(Sf2File {
            header: header.unwrap_or(SoundFontHeader {
                name: String::new(),
                rom_name: String::new(),
                rom_version: (0, 0),
                internal_version: (0, 0),
                software: String::new(),
                sample_rate: 44100,
            }),
            smpl_data: smpl_arc,
            samples,
            instruments,
            presets,
        })
    }

    /// 4CC (4바이트 문자 코드) 읽기
    fn read_four_cc<R: Read>(reader: &mut R) -> Result<[u8; 4], ParseError> {
        let mut id = [0u8; 4];
        reader.read_exact(&mut id)?;
        Ok(id)
    }

    /// INFO 청크 파싱
    fn parse_info_chunk<R: Read + Seek>(
        reader: &mut R,
        info_size: u32,
    ) -> Result<SoundFontHeader, ParseError> {
        let mut name = String::new();
        let mut rom_name = String::new();
        let mut rom_version = (0u16, 0u16);
        let mut internal_version = (0u16, 0u16);
        let mut software = String::new();
        let mut sample_rate = 44100u32;

        let mut consumed: u32 = 0;
        while consumed + 8 <= info_size {
            let chunk_id = Self::read_four_cc(reader);
            if chunk_id.is_err() {
                break;
            }
            let chunk_id = chunk_id.unwrap();
            let chunk_size = reader.read_u32::<LittleEndian>()?;
            consumed += 8;

            if chunk_size > 1024 * 1024 {
                break;
            }

            match chunk_id.as_ref() {
                b"ifil" => {
                    let major = reader.read_u16::<LittleEndian>()?;
                    let minor = reader.read_u16::<LittleEndian>()?;
                    internal_version = (major, minor);
                    consumed += chunk_size;
                    // 2바이트 정렬
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                        consumed += 1;
                    }
                }
                b"INAM" => {
                    name = Self::read_pstring(reader, chunk_size as usize)?;
                    consumed += chunk_size;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                        consumed += 1;
                    }
                }
                b"irom" => {
                    rom_name = Self::read_pstring(reader, chunk_size as usize)?;
                    consumed += chunk_size;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                        consumed += 1;
                    }
                }
                b"iver" => {
                    let major = reader.read_u16::<LittleEndian>()?;
                    let minor = reader.read_u16::<LittleEndian>()?;
                    rom_version = (major, minor);
                    consumed += chunk_size;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                        consumed += 1;
                    }
                }
                b"isng" => {
                    software = Self::read_pstring(reader, chunk_size as usize)?;
                    consumed += chunk_size;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                        consumed += 1;
                    }
                }
                b"srfo" => {
                    sample_rate = reader.read_u32::<LittleEndian>()?;
                    consumed += chunk_size;
                }
                _ => {
                    reader.seek(SeekFrom::Current(chunk_size as i64))?;
                    consumed += chunk_size;
                    if chunk_size % 2 == 1 {
                        reader.seek(SeekFrom::Current(1))?;
                        consumed += 1;
                    }
                }
            }
        }

        Ok(SoundFontHeader {
            name,
            rom_name,
            rom_version,
            internal_version,
            software,
            sample_rate,
        })
    }

    /// 패딩된 문자열 읽기
    fn read_pstring<R: Read>(reader: &mut R, size: usize) -> Result<String, ParseError> {
        let mut buf = vec![0u8; size];
        reader.read_exact(&mut buf)?;
        // NULL 문자 제거
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(end);
        // 패딩 공백 제거
        while let Some(&last) = buf.last() {
            if last == b' ' || last == 0 {
                buf.pop();
            } else {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    /// shdr + smpl 파싱
    fn parse_shdr_and_samples(
        shdr_data: &[u8],
        smpl_data: &Arc<Vec<i16>>,
    ) -> Result<Vec<Sample>, ParseError> {
        if shdr_data.is_empty() {
            if !smpl_data.is_empty() {
                return Ok(vec![Sample {
                    name: "Default".to_string(),
                    start: 0,
                    end: smpl_data.len() as u32,
                    start_loop: 0,
                    end_loop: smpl_data.len() as u32,
                    sample_rate: 44100,
                    original_pitch: 60,
                    pitch_correction: 0,
                    sample_link: 0,
                    sample_type: SampleType::Mono,
                }]);
            }
            return Ok(Vec::new());
        }

        let record_size = 46; // shdr 레코드 크기
        let count = shdr_data.len() / record_size;
        let mut samples = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * record_size;
            let record = &shdr_data[offset..offset + record_size];
            let mut cursor = std::io::Cursor::new(record);

            let mut name_buf = [0u8; 20];
            cursor.read_exact(&mut name_buf)?;
            let name = Self::extract_string(&name_buf);

            let start = cursor.read_u32::<LittleEndian>()?;
            let end = cursor.read_u32::<LittleEndian>()?;
            let start_loop = cursor.read_u32::<LittleEndian>()?;
            let end_loop = cursor.read_u32::<LittleEndian>()?;
            let sample_rate = cursor.read_u32::<LittleEndian>()?;
            let original_pitch = cursor.read_u8()?;
            let pitch_correction = cursor.read_i8()?;
            let sample_link = cursor.read_u16::<LittleEndian>()?;
            let sample_type_val = cursor.read_u16::<LittleEndian>()?;

            let sample_type = match sample_type_val {
                1 => SampleType::Mono,
                2 => SampleType::Right,
                4 => SampleType::Left,
                8 => SampleType::Linked,
                0x8001 => SampleType::RomMono,
                0x8002 => SampleType::RomRight,
                0x8004 => SampleType::RomLeft,
                0x8008 => SampleType::RomLinked,
                _ => SampleType::Mono,
            };

            // 샘플 데이터는 Sf2File.smpl_data에 저장되어 있고
            // start/end 인덱스로 참조만 한다
            samples.push(Sample {
                name,
                start,
                end,
                start_loop,
                end_loop,
                sample_rate,
                original_pitch,
                pitch_correction,
                sample_link,
                sample_type,
            });
        }

        // 끝 레코드 제거
        if samples.len() > 1 {
            samples.pop();
        }

        Ok(samples)
    }

    /// 프리셋/악기 데이터 처리
    fn process_preset_data(
        phdr_data: &[u8],
        pbag_data: &[u8],
        pgen_data: &[u8],
        ihdr_data: &[u8],
        ibag_data: &[u8],
        igen_data: &[u8],
        sample_count: usize,
    ) -> Result<(Vec<Preset>, Vec<Instrument>), ParseError> {
        // 악기 파싱
        let instruments = Self::parse_instruments(ihdr_data, ibag_data, igen_data, sample_count)?;

        // 프리셋 파싱
        let presets = Self::parse_presets(phdr_data, pbag_data, pgen_data, instruments.len())?;

        Ok((presets, instruments))
    }

    /// 악기 파싱
    fn parse_instruments(
        ihdr_data: &[u8],
        ibag_data: &[u8],
        igen_data: &[u8],
        sample_count: usize,
    ) -> Result<Vec<Instrument>, ParseError> {
        let record_size = 22; // ihdr 레코드 크기
        let count = ihdr_data.len() / record_size;
        let mut instruments = Vec::with_capacity(count);

        // ihdr에서 악기 이름 추출
        let mut inst_names: Vec<String> = Vec::new();
        for i in 0..count {
            let offset = i * record_size;
            let name_buf = &ihdr_data[offset..offset + 20];
            inst_names.push(Self::extract_string(&name_buf.try_into().unwrap_or([0; 20])));
        }
        if !inst_names.is_empty() {
            inst_names.pop(); // 끝 레코드 제거
        }

        // ibag 파싱 (존 범위)
        let bag_record_size = 4; // pbag/ibag 레코드 크기
        let bag_count = ibag_data.len() / bag_record_size;

        // igen 파싱 (제네레이터)
        let gen_record_size = 4; // pgen/igen 레코드 크기

        for (i, name) in inst_names.iter().enumerate() {
            let start_bag = if i == 0 { 0 } else {
                if i * bag_record_size < ibag_data.len() {
                    let bag_idx = i - 1;
                    let offset = bag_idx * bag_record_size;
                    let mut cursor = std::io::Cursor::new(ibag_data);
                    cursor.seek(SeekFrom::Current(offset as i64)).ok();
                    cursor.read_u16::<LittleEndian>().unwrap_or(0) as usize
                } else { 0 }
            };
            let end_bag = if (i + 1) * bag_record_size < ibag_data.len() {
                let offset = i * bag_record_size;
                let mut cursor = std::io::Cursor::new(ibag_data);
                cursor.seek(SeekFrom::Current(offset as i64)).ok();
                cursor.read_u16::<LittleEndian>().unwrap_or(0) as usize
            } else {
                bag_count
            };

            let mut zones = Vec::new();
            for bag_idx in start_bag..end_bag {
                let bag_offset = bag_idx * bag_record_size;
                if bag_offset + bag_record_size > ibag_data.len() {
                    break;
                }

                let mut cursor = std::io::Cursor::new(ibag_data);
                cursor.seek(SeekFrom::Current(bag_offset as i64)).ok();
                // ibag: (wGenNdx, wModNdx) - gen 범위는 [ibag[i].wGenNdx .. ibag[i+1].wGenNdx)
                let gen_start = cursor.read_u16::<LittleEndian>().unwrap_or(0) as usize;
                let gen_end = if bag_idx + 1 < bag_count {
                    let next_bag_offset = (bag_idx + 1) * bag_record_size;
                    let mut c2 = std::io::Cursor::new(ibag_data);
                    c2.seek(SeekFrom::Current(next_bag_offset as i64)).ok();
                    c2.read_u16::<LittleEndian>().unwrap_or(0) as usize
                } else {
                    igen_data.len() / 4
                };
                cursor.read_u16::<LittleEndian>().ok(); // wModNdx 건너뛰기

                // igen에서 샘플 인덱스 찾기
                let mut sample_index = None;
                let mut key_range = (0u8, 127u8);
                let mut velocity_range = (0u8, 127u8);

                for gen_idx in gen_start..gen_end {
                    let gen_offset = gen_idx * gen_record_size;
                    if gen_offset + gen_record_size > igen_data.len() {
                        break;
                    }

                    let mut cursor = std::io::Cursor::new(igen_data);
                    cursor.seek(SeekFrom::Current(gen_offset as i64)).ok();
                    let gen_type = cursor.read_u16::<LittleEndian>().unwrap_or(0);
                    let gen_val = cursor.read_i16::<LittleEndian>().unwrap_or(0);

                    match gen_type {
                        53 => {
                            // SampleID (instrument zone이 가리키는 샘플)
                            if gen_val >= 0 && (gen_val as usize) < sample_count {
                                sample_index = Some(gen_val as usize);
                            }
                        }
                        44 => {
                            // KeyRange
                            // 로우 바이트가 로우, 하이 바이트가 하이
                            key_range = (gen_val as u8, (gen_val >> 8) as u8);
                        }
                        45 => {
                            // VelocityRange
                            velocity_range = (gen_val as u8, (gen_val >> 8) as u8);
                        }
                        _ => {}
                    }
                }

                zones.push(InstrumentZone {
                    sample_index,
                    key_range,
                    velocity_range,
                    generators: Vec::new(),
                });
            }

            instruments.push(Instrument {
                name: name.clone(),
                zones,
            });
        }

        Ok(instruments)
    }

    /// 프리셋 파싱
    fn parse_presets(
        phdr_data: &[u8],
        pbag_data: &[u8],
        pgen_data: &[u8],
        instrument_count: usize,
    ) -> Result<Vec<Preset>, ParseError> {
        let record_size = 38; // phdr 레코드 크기
        let count = phdr_data.len() / record_size;
        let mut presets = Vec::with_capacity(count);

        // pbag 파싱
        let bag_record_size = 4;
        let bag_count = pbag_data.len() / bag_record_size;

        // pgen 파싱
        let gen_record_size = 4;

        for i in 0..count {
            let offset = i * record_size;
            let record = &phdr_data[offset..offset + record_size];
            let mut cursor = std::io::Cursor::new(record);

            let mut name_buf = [0u8; 20];
            cursor.read_exact(&mut name_buf)?;
            let name = Self::extract_string(&name_buf);

            let preset_num = cursor.read_u16::<LittleEndian>()?;
            let bank = cursor.read_u16::<LittleEndian>()?;
            let preset_bag_index = cursor.read_u16::<LittleEndian>()?;

            // 끝 마커 확인
            if preset_num == 0xFFFF && bank == 0xFFFF {
                break;
            }

            // 존 범위 계산
            let start_bag = preset_bag_index as usize;
            let end_bag = if i + 1 < count {
                let next_offset = (i + 1) * record_size;
                let next_record = &phdr_data[next_offset..next_offset + record_size];
                let mut next_cursor = std::io::Cursor::new(next_record);
                let mut next_name_buf = [0u8; 20];
                next_cursor.read_exact(&mut next_name_buf).ok();
                next_cursor.read_u16::<LittleEndian>().ok();
                next_cursor.read_u16::<LittleEndian>().ok();
                next_cursor.read_u16::<LittleEndian>().unwrap_or(bag_count as u16) as usize
            } else {
                bag_count
            };

            let mut zones = Vec::new();
            for bag_idx in start_bag..end_bag {
                let bag_offset = bag_idx * bag_record_size;
                if bag_offset + bag_record_size > pbag_data.len() {
                    break;
                }

                let mut cursor = std::io::Cursor::new(pbag_data);
                cursor.seek(SeekFrom::Current(bag_offset as i64)).ok();
                // pbag: (wGenNdx, wModNdx) - gen 범위는 [pbag[i].wGenNdx .. pbag[i+1].wGenNdx)
                let gen_start = cursor.read_u16::<LittleEndian>().unwrap_or(0) as usize;
                let gen_end = if bag_idx + 1 < bag_count {
                    let next_bag_offset = (bag_idx + 1) * bag_record_size;
                    let mut c2 = std::io::Cursor::new(pbag_data);
                    c2.seek(SeekFrom::Current(next_bag_offset as i64)).ok();
                    c2.read_u16::<LittleEndian>().unwrap_or(0) as usize
                } else {
                    pgen_data.len() / gen_record_size
                };
                cursor.read_u16::<LittleEndian>().ok(); // wModNdx 건너뛰기

                // pgen에서 악기 인덱스 찾기
                let mut instrument_index = None;
                let mut key_range = (0u8, 127u8);
                let mut velocity_range = (0u8, 127u8);

                for gen_idx in gen_start..gen_end {
                    let gen_offset = gen_idx * gen_record_size;
                    if gen_offset + gen_record_size > pgen_data.len() {
                        break;
                    }

                    let mut cursor = std::io::Cursor::new(pgen_data);
                    cursor.seek(SeekFrom::Current(gen_offset as i64)).ok();
                    let gen_type = cursor.read_u16::<LittleEndian>().unwrap_or(0);
                    let gen_val = cursor.read_i16::<LittleEndian>().unwrap_or(0);

                    match gen_type {
                        41 => {
                            // Instrument (인덱스)
                            let inst_idx = gen_val as usize;
                            if inst_idx < instrument_count {
                                instrument_index = Some(inst_idx);
                            }
                        }
                        44 => {
                            // KeyRange
                            key_range = (gen_val as u8, (gen_val >> 8) as u8);
                        }
                        45 => {
                            // VelocityRange
                            velocity_range = (gen_val as u8, (gen_val >> 8) as u8);
                        }
                        _ => {}
                    }
                }

                if instrument_index.is_none() && bag_idx < 3 {
                    // 디버그: 첫 bag instrument_index None (이건 정상일 수 있음)
                }

                zones.push(PresetZone {
                    instrument_index,
                    key_range,
                    velocity_range,
                    generators: Vec::new(),
                });
            }

            presets.push(Preset {
                name,
                bank,
                preset_num,
                zones,
            });
        }

        Ok(presets)
    }

    /// 문자열 버퍼에서 문자열 추출
    fn extract_string(buf: &[u8; 20]) -> String {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).trim().to_string()
    }
}

/// 파싱된 SF2 파일
#[derive(Debug, Clone)]
pub struct Sf2File {
    /// 헤더 정보
    pub header: SoundFontHeader,
    /// 샘플 데이터 (모든 샘플이 공유하는 원본 PCM 데이터, Arc로 참조 카운팅)
    pub smpl_data: Arc<Vec<i16>>,
    /// 샘플 목록
    pub samples: Vec<Sample>,
    /// 악기 목록
    pub instruments: Vec<Instrument>,
    /// 프리셋 목록
    pub presets: Vec<Preset>,
}
