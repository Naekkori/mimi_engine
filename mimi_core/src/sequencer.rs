pub use crate::rhythm_engine::BsChordEvent;
use midly::{Smf, TrackEventKind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiFormat {
    GM,
    GS,
    XG,
}

#[derive(Debug, Clone)]
pub enum MidiEngineEvent {
    MidiPlay {
        port: u8,
        channel: u8,
        is_drum_channel: bool,
        kind: TrackEventKind<'static>,
    },
    TempoChange {
        tempo: u32,
    },
    // 노래방 가사 (내장)
    SmfKaraokeText {
        text: String,
    },
    //시스템 리셋
    MidiReset,
    //재생 진행 상태
    TickUpdate {
        current_tick: u64,
        total_tick: u64,
    },
    ChannelLevel {
        port: u8,
        levels: [u8; 16],
    },
    // 드럼 채널 설정 변경
    SetDrumChannel {
        port: u8,
        channel: u8,
        is_drum: bool,
    },
    // 엔진에 의해 분석되고 적용 중인 실시간 코드 상태 정보 전송 (디버그용 UI 브리징)
    ChordUpdate {
        root_pitch: u8, // C=0, C#=1 ... B=11
        is_minor: bool,
        is_7th: bool,
        is_maj7: bool,
    },
    // 리듬엔진 제어
    RhythmEngineControl {
        command: u8, //커멘드 타입
        mask_lo: u8, //하위 8비트 마스크 (채널 1~8)
        mask_hi: u8, //상위 8비트 마스크 (채널 9~16)
    },
    // Fluidsynth/Fluidlite 에러 및 경고 메세지 전송 채널
    FluidsynthWarning {
        message: String,
    },
    //음정 (미디파일 에 정의되어있는 음정)
    KeySignature {
        key: i8,
        is_sharp: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SequenceEvent {
    pub absolute_tick: u32,
    pub priority: u8, // 0: Meta/Setup, 1: Note
    pub inner: MidiEngineEvent,
}

pub struct MimiSequencer {
    pub event: Vec<SequenceEvent>,
    pub ppq: u16,
    pub current_event_index: usize,
    pub current_tick: f64,
    pub microseconds_per_tick: f64,
    pub total_ticks: u32,
    pub format: MidiFormat,

    // 리듬엔진의 실시간 코드 변환을 위한 $BS(Bass) 타임라인
    pub chord_timeline: Vec<BsChordEvent>,
    // 멜로디(가이드) 채널 (포트번호, 채널번호) 목록
    pub melody_channels: Vec<(u8, u8)>,
    // 실제 미디 파일 트랙에서 $BS (또는 bass 명시 트랙)이 감지되었는지 여부
    pub is_bs_track_detected: bool,
    // 가이드 멜로디 피치 분석 결과 곡의 성별 (true: 여성, false: 남성, None: 분석 불가)
    pub is_female: Option<bool>,
}

impl MimiSequencer {
    pub fn empty() -> Self {
        Self {
            event: Vec::new(),
            ppq: 480,
            current_event_index: 0,
            current_tick: 0.0,
            microseconds_per_tick: 500_000.0 / 480.0,
            total_ticks: 0,
            format: MidiFormat::GM,
            chord_timeline: Vec::new(),
            melody_channels: Vec::new(),
            is_bs_track_detected: false,
            is_female: None,
        }
    }

    pub fn from_byte(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        let smf = Smf::parse(bytes).map_err(|e| anyhow::anyhow!("SMF 파싱 실패: {:?}", e))?;

        let ppq = match smf.header.timing {
            midly::Timing::Metrical(ticks) => ticks.as_int(),
            _ => 480,
        };

        let mut all_events = Vec::new();
        let mut detected_format = MidiFormat::GM;
        let mut chord_timeline: Vec<BsChordEvent> = Vec::new();
        let mut melody_channels: Vec<(u8, u8)> = Vec::new();
        let mut melody_notes = Vec::new();

        // 가사 이벤트를 수집하여 $BS 베이스 라인이 아예 없을 때의 폴백용 코드 구조
        let mut lyric_chords: Vec<(u32, String)> = Vec::new();

        // 1번 혹은 4번 채널은 노래방에서 기본 멜로디 채널로 자주 쓰임
        melody_channels.push((0, 0)); // Ch 0
        melody_channels.push((0, 3)); // Ch 3 (0-based)

        // 미디 파일 전체 멀티트랙들을 로드해서 파싱 연주 루프에 돌입하기 전,
        // 전체 트랙 헤더 메타 영역을 1차 선제 탐색하여 트랙명이 "$BS" 인 물리적인 실제 트랙 인덱스 목록을 사전 확보한다.
        // 추가로 트랙명 내에 key- 가 기재된 경우 실제 원곡 키 정보를 파싱하여 보관한다.
        let mut bs_track_indices = Vec::new();
        let mut parsed_key_sig = None;
        for (track_idx, track) in smf.tracks.iter().enumerate() {
            for event in track.iter() {
                // 트랙의 델타타임 누적과 무관하게 맨 앞 트랙이름 메타데이터만 1회성 스캔
                if let midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(bytes)) =
                    &event.kind
                {
                    let decoded_str = String::from_utf8_lossy(bytes).to_lowercase();
                    let name_bytes = *bytes;

                    // key- 또는 key= 패턴 파싱
                    if parsed_key_sig.is_none() {
                        if let Some(idx) = decoded_str.find("key-").or_else(|| decoded_str.find("key=")) {
                            let start = idx + 4;
                            let mut key_str = String::new();
                            for c in decoded_str[start..].chars() {
                                if c.is_ascii_alphabetic() || c == '#' {
                                    key_str.push(c);
                                } else {
                                    break;
                                }
                            }
                            if !key_str.is_empty() {
                                if let Some(sig) = parse_key_str(&key_str) {
                                    parsed_key_sig = Some(sig);
                                }
                            }
                        }
                    }

                    let mut has_bs_tag = decoded_str.contains("$bs");

                    // CP949 깨짐 노이즈 대비 바이트 스캔
                    if !has_bs_tag && name_bytes.len() >= 2 {
                        for i in 0..=(name_bytes.len() - 2) {
                            let b1 = name_bytes[i].to_ascii_lowercase();
                            let b2 = name_bytes[i + 1].to_ascii_lowercase();
                            if b1 == b'b' && b2 == b's' {
                                has_bs_tag = true;
                                break;
                            }
                        }
                    }
                    if !has_bs_tag && name_bytes.len() >= 4 {
                        for i in 0..=(name_bytes.len() - 4) {
                            let b1 = name_bytes[i].to_ascii_lowercase();
                            let b2 = name_bytes[i + 1].to_ascii_lowercase();
                            let b3 = name_bytes[i + 2].to_ascii_lowercase();
                            let b4 = name_bytes[i + 3].to_ascii_lowercase();
                            if b1 == b'b' && b2 == b'a' && b3 == b's' && b4 == b's' {
                                has_bs_tag = true;
                                break;
                            }
                        }
                    }

                    if has_bs_tag {
                        bs_track_indices.push(track_idx);
                        break;
                    }
                }
            }
        }

        // 멀티트랙 미디파일을 단일절대 틱 타임라인으로 병합
        let is_bs_track_detected = !bs_track_indices.is_empty();

        for (track_idx, track) in smf.tracks.iter().enumerate() {
            let mut accum_tick = 0u32;
            let mut current_port = 0u8;
            let is_bass_track = bs_track_indices.contains(&track_idx);
            let mut track_bass_notes: Vec<(u32, u8)> = Vec::new();

            // Conductor 및 $BS, $GS, $RS, $FS 트랙들은 화면상 Port P (실제 미디 포트 번호 15 혹은 특정 디바이스 규격)로 출력됨
            // midly에서는 MidiPort(port)가 u8 형식이므로 0~15 가이드라인 가능.
            // 특히 Port P, Port C, Port N 등으로 표기되는 반주기 메타데이터 대응

            for event in track.iter() {
                accum_tick += event.delta.as_int();

                // SysEx는 to_static() 호출 시 바이트슬라이스가 빈 슬라이스로 교체되므로
                // 원본 event.kind에서 직접 파싱해야 함
                if let TrackEventKind::SysEx(data) = &event.kind {
                    let is_gm_reset = data.len() >= 4 && data[0] == 0x7E && data[2] == 0x09;

                    let is_gs_reset = data.len() >= 7
                        && data[0] == 0x41
                        && data[2] == 0x42
                        && data[3] == 0x12
                        && data[4] == 0x40
                        && data[5] == 0x00
                        && data[6] == 0x7F;

                    let is_xg_on = data.len() >= 7
                        && data[0] == 0x43
                        && data[2] == 0x4C
                        && data[3] == 0x00
                        && data[4] == 0x00
                        && data[5] == 0x7E
                        && data[6] == 0x00;

                    if is_xg_on {
                        detected_format = MidiFormat::XG;
                    } else if is_gs_reset {
                        detected_format = MidiFormat::GS;
                    } else if is_gm_reset {
                        detected_format = MidiFormat::GM;
                    }

                    if is_gm_reset || is_gs_reset || is_xg_on {
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 0,
                            inner: MidiEngineEvent::MidiReset,
                        });
                    }

                    // Roland GS Rhythm Part Assign
                    // F0 41 <device_id> 42 12 40 1x 15 <value> <checksum> F7
                    // F0, F7 제거된 data 바이트 기준
                    if data.len() >= 9
                        && data[0] == 0x41
                        && data[2] == 0x42
                        && data[3] == 0x12
                        && data[4] == 0x40
                        && (data[5] & 0xF0) == 0x10
                        && data[6] == 0x15
                    {
                        let part = data[5] & 0x0F;
                        let ch = match part {
                            0 => 9,                         // Part 10 -> Ch 10 (0-based 9)
                            p if p >= 1 && p <= 9 => p - 1, // Part 1~9 -> Ch 1~9 (0-based 0~8)
                            p if p >= 10 && p <= 15 => p, // Part 11~16 -> Ch 11~16 (0-based 10~15)
                            _ => 9,
                        };
                        let is_drum = data[7] == 1 || data[7] == 2;
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 1,
                            inner: MidiEngineEvent::SetDrumChannel {
                                port: current_port,
                                channel: ch,
                                is_drum,
                            },
                        });
                    }

                    // Yamaha XG Part Mode (Rhythm Part Assign)
                    // F0 43 <device_id> 4C 08 1x 0E <value> F7
                    // F0, F7 제거된 data 바이트 기준
                    if data.len() >= 7
                        && data[0] == 0x43
                        && data[2] == 0x4C
                        && data[3] == 0x08
                        && (data[4] & 0xF0) == 0x10
                        && data[5] == 0x0E
                    {
                        let part = data[4] & 0x0F;
                        let is_drum = data[6] >= 1;
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 1,
                            inner: MidiEngineEvent::SetDrumChannel {
                                port: current_port,
                                channel: part,
                                is_drum,
                            },
                        });
                    }
                    // 리듬엔진 제어 메세지
                    /*
                       바이트 순서,값 (Hex),설명
                       1,0xF0,시작
                       2,0x7D,벤더 코드
                       3,0x01,시스템 제어
                       4,0x20,포트 A 리듬 변환 모드 (0x01: 리듬 변환, 0x00: 리듬 변환 해제)
                       5,0x9F,채널 1~8 뮤트 마스크
                       6,0xFF,채널 9~16 뮤트 마스크
                       7,0xF7,종료
                    */
                    if data.len() >= 5
                        && data[0] == 0x7D
                        && (data[1] == 0x01 || data[1] == 0x00)
                        && data[2] == 0x20
                    {
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 1,
                            inner: MidiEngineEvent::RhythmEngineControl {
                                command: data[1],
                                mask_lo: data[3],
                                mask_hi: data[4],
                            },
                        });
                    }
                }

                let kind = event.kind.to_static();

                match &kind {
                    TrackEventKind::Meta(midly::MetaMessage::TrackName(bytes)) => {
                        let decoded_str = String::from_utf8_lossy(bytes).to_lowercase();
                        if decoded_str.contains("melody")
                            || decoded_str.contains("vocal")
                            || decoded_str.contains("guide")
                        {
                            // 기본 보컬 멜로디 가이드라인 채널 매칭
                        }
                    }
                    TrackEventKind::Meta(midly::MetaMessage::MidiPort(port)) => {
                        // 포트 번호 그대로 유지 (0, 1 외에 2 이상의 반주기 포트번호도 정상 맵핑)
                        current_port = port.as_int();
                    }
                    TrackEventKind::Meta(midly::MetaMessage::Lyric(bytes))
                    | TrackEventKind::Meta(midly::MetaMessage::Text(bytes)) => {
                        let text = String::from_utf8_lossy(bytes).to_string();
                        if !text.is_empty() && !text.starts_with('@') {
                            // 가사 텍스트에 포함된 코드네임 정보 임시 추출 수집
                            lyric_chords.push((accum_tick, text.clone()));

                            all_events.push(SequenceEvent {
                                absolute_tick: accum_tick,
                                priority: 0,
                                inner: MidiEngineEvent::SmfKaraokeText { text },
                            });
                        }
                    }
                    // 키 시그니처
                    TrackEventKind::Meta(midly::MetaMessage::KeySignature(key, is_sharp)) => {
                        let (final_key, final_is_sharp) = if let Some((pk, pim)) = parsed_key_sig {
                            (pk, pim)
                        } else {
                            (*key, *is_sharp)
                        };
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 0,
                            inner: MidiEngineEvent::KeySignature {
                                key: final_key,
                                is_sharp: final_is_sharp,
                            },
                        });
                    }

                    TrackEventKind::Midi { channel, message } => {
                        let ch_byte = u8::from(*channel);
                        let is_drum = ch_byte == 9;
                        let priority: u8 = match message {
                            midly::MidiMessage::ProgramChange { .. } => 1,
                            midly::MidiMessage::NoteOn { .. }
                            | midly::MidiMessage::NoteOff { .. } => 2,
                            _ => 0,
                        };

                        // [중요] 노래방 반주기 전용 포트 감지 필터 수정:
                        // 사전 선제 조사 단계에서 $BS 트랙을 확실하게 발견한 경우에는
                        // 엉뚱한 베이스 채널(Ch 1, 10)이나 타 포트 음정 노이즈가 섞이지 않도록
                        // 철저하게 해당 $BS 트랙(is_bass_track == true)의 노트 정보만 코드 연주 및 추출용 베이스라인으로 활용한다.
                        // $BS 트랙이 전혀 발견되지 않은 공미디 파일인 경우에만 포트15, 채널1/10 등 기본 검출 모드로 폴백 기동한다.
                        let is_bs_track_or_port = if is_bs_track_detected {
                            is_bass_track
                        } else {
                            ch_byte == 1 || ch_byte == 10 || current_port == 15 || current_port == 1
                        };

                        // 베이스 또는 $BS 트랙인 경우 음정 데이터 수집
                        if is_bs_track_or_port && !is_drum {
                            if let midly::MidiMessage::NoteOn { key, vel } = message {
                                if vel.as_int() > 0 {
                                    track_bass_notes.push((accum_tick, key.as_int()));
                                }
                            }
                        }

                        // 멜로디 채널 노트 수집
                        if melody_channels.contains(&(current_port, ch_byte)) && !is_drum {
                            if let midly::MidiMessage::NoteOn { key, vel } = message {
                                if vel.as_int() > 0 {
                                    melody_notes.push(key.as_int());
                                }
                            }
                        }

                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority,
                            inner: MidiEngineEvent::MidiPlay {
                                port: current_port,
                                channel: ch_byte,
                                is_drum_channel: is_drum,
                                kind: TrackEventKind::Midi {
                                    channel: *channel,
                                    message: *message,
                                },
                            },
                        })
                    }
                    // 템포 변경 이벤트도 시퀀서가 트래킹할 수 있도록 포함
                    TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo)) => {
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 0,
                            inner: MidiEngineEvent::TempoChange {
                                tempo: tempo.as_int(),
                            },
                        });
                    }
                    // SysEx 처리는 위의 event.kind 직접 참조 블록에서 완료됨
                    TrackEventKind::SysEx(_) => {}
                    _ => {}
                }
            }
            // 트랙 단위 파싱이 끝난 직후 베이스 데이터가 있으면 분석 수행
            if !track_bass_notes.is_empty() {
                for &(tick, key) in &track_bass_notes {
                    let root = key % 12; // C=0, C#=1... 정규화

                    // 장음계/단음계 분기 처리 (기본으로 장3도 성분 포함되어 있는지 유추)
                    // (노래방 미디의 경우 복잡한 코드 텐션을 단시간에 전부 추출할 수 없어,
                    // 베이스 음정 변화를 C Major 스케일 기준으로 간단히 마킹)
                    let is_minor = match root {
                        2 | 9 | 11 => true, // Dm, Am, Bm 계열 스케일 약식 보정
                        _ => false,
                    };
                    chord_timeline.push(BsChordEvent {
                        tick,
                        root_pitch: root,
                        is_minor,
                        is_7th: false, // 베이스 멜로디에서는 7도 유추가 어려우므로 기본값 처리함
                        is_maj7: false,
                    });
                }
            }
        }

        // 만약 $BS 트랙이나 베이스 라인 노트를 전혀 추출하지 못했다면 가사 기반 폴백 진행
        if chord_timeline.is_empty() && !lyric_chords.is_empty() {
            // 가사 문자열 중 전형적인 서양식 코드 스크립트([C], [Am], [G7], C, F, G 등 단독 기재) 추출
            // 정규식이나 무겁게 쓰지 않고 단순 토큰 기반의 키 매핑 파서 구현
            for (tick, text) in lyric_chords {
                // 대괄호를 벗기거나 정리
                let cleaned = text
                    .trim()
                    .replace('[', "")
                    .replace(']', "")
                    .replace('(', "")
                    .replace(')', "");

                // 공백 기준으로 잘라 가사 사이에 코드 토큰이 있는지 검색
                for word in cleaned.split_whitespace() {
                    if let Some((root, is_minor, is_7th, is_maj7)) = parse_single_chord_name(word) {
                        chord_timeline.push(BsChordEvent {
                            tick,
                            root_pitch: root,
                            is_minor,
                            is_7th,
                            is_maj7,
                        });
                        break; // 한 가사 토큰 라인에서는 첫 번째 매치된 코드만 채택
                    }
                }
            }
        }

        // 만약 KeySignature 메타 이벤트가 병합된 이벤트 목록에 하나도 없다면,
        // 파싱된 키(parsed_key_sig)가 존재할 때 0틱 시점에 강제로 KeySignature 이벤트를 넣어줌
        if let Some((pk, pim)) = parsed_key_sig {
            let has_key_sig = all_events.iter().any(|e| matches!(e.inner, MidiEngineEvent::KeySignature { .. }));
            if !has_key_sig {
                all_events.push(SequenceEvent {
                    absolute_tick: 0,
                    priority: 0,
                    inner: MidiEngineEvent::KeySignature {
                        key: pk,
                        is_sharp: pim,
                    },
                });
            }
        }

        // 절대틱 오름차순, 같은 틱이면 우선순위(priority) 오름차순으로 정렬
        all_events.sort_by(|a, b| {
            a.absolute_tick
                .cmp(&b.absolute_tick)
                .then(a.priority.cmp(&b.priority))
        });

        // chord_timeline도 시간 기준 정렬
        chord_timeline.sort_by_key(|c| c.tick);

        // 마지막 이벤트의 절대틱을 총 틱으로 설정.
        let total_ticks = all_events.last().map(|e| e.absolute_tick).unwrap_or(0);
        // 초기템포 설정
        let initial_per_beat = 500_000.0;
        let microseconds_per_tick = initial_per_beat / ppq as f64;

        // 가이드 멜로디 피치를 기반으로 성별 판정
        let is_female = if !melody_notes.is_empty() {
            let max_note = *melody_notes.iter().max().unwrap();
            let sum_note: u64 = melody_notes.iter().map(|&n| n as u64).sum();
            let avg_note = sum_note as f64 / melody_notes.len() as f64;
            // 최고음이 C5(72) 이상이거나 평균음이 F#4(66) 이상이면 여성 키로 판정
            Some(max_note >= 72 || avg_note >= 66.0)
        } else {
            None
        };

        Ok(Self {
            event: all_events,
            ppq,
            current_event_index: 0,
            current_tick: 0.0,
            microseconds_per_tick,
            total_ticks,
            format: detected_format,
            chord_timeline,
            melody_channels,
            is_bs_track_detected,
            is_female,
        })
    }
    //시간 경과에 따라 틱을 전진시키고
    //해당 시점에 실행되어야 하는 미디이벤트 목록을 추출하여 반환
    pub fn marching(&mut self, delta_sec: f64, tempo_scale: f32) -> Vec<SequenceEvent> {
        let mut triggered = Vec::new();

        //템포 스케일(배속) 이 반영된 델타시간 계산
        let delta_microsec = (delta_sec * 1_000_000.0) * tempo_scale as f64;

        //경과 시간에 따라 몇 틱 을 전진해야 하는지 계산
        let delta_ticks = delta_microsec / self.microseconds_per_tick;
        self.current_tick += delta_ticks;

        // 총 틱을 초과하지 않도록 제한 (오버런 방지)
        if self.current_tick > self.total_ticks as f64 {
            self.current_tick = self.total_ticks as f64;
        }

        //현재 틱 위치까지 도달한 이벤트 전부 가져옴
        while self.current_event_index < self.event.len() {
            let event = &self.event[self.current_event_index];
            // 부동 소수점 오차를 고려하여 아주 미세한 여유값(0.0001)을 더해 비교
            if (event.absolute_tick as f64) <= self.current_tick + 0.0001 {
                // 템포 변경 이벤트 대응 (분리된 TempoChange variant 처리)
                if let MidiEngineEvent::TempoChange { tempo } = &event.inner {
                    let per_beat = *tempo as f64;
                    self.microseconds_per_tick = per_beat / self.ppq as f64;
                }

                triggered.push(event.clone());
                self.current_event_index += 1;
            } else {
                break;
            }
        }
        triggered
    }

    //처음으로 되돌리기
    pub fn reset(&mut self) {
        self.current_event_index = 0;
        self.current_tick = 0.0;
        self.microseconds_per_tick = 500_000.0 / self.ppq as f64;
    }

    // 모든 이벤트가 실행됬는가
    pub fn is_finished(&self) -> bool {
        // 모든 이벤트가 이미 처리(발송)되었다면 연주는 끝난 것으로 간주함
        // current_tick 조건보다 index 조건이 더 확실한 종료 신호임
        self.current_event_index >= self.event.len()
    }

    // 지정된 틱 위치 이후 첫 이벤트 인덱스를 반환
    pub fn find_next_event_index(&self, tick: u32) -> usize {
        self.event.partition_point(|e| e.absolute_tick < tick)
    }

    // 지정된 틱 위치로 점프하고, 해당 지점까지 템포 를 복원
    pub fn seek_to(&mut self, tick: u32) {
        self.current_tick = tick as f64;
        self.current_event_index = self.find_next_event_index(tick);

        // 틱 이전의 마지막 템포 이벤트를 찿아서 복원
        let mut last_tempo = 500_000.0;
        for i in 0..self.current_event_index {
            if let MidiEngineEvent::TempoChange { tempo } = &self.event[i].inner {
                last_tempo = *tempo as f64;
            }
        }
        self.microseconds_per_tick = last_tempo / self.ppq as f64;
    }
}

/// 가사 텍스트에 개별 존재할 수 있는 단일 코드(예: [C], Am, F#m, Bb7)를 분석하여
/// 정규화된 root_pitch(0~11), minor 여부, 7th 여부, maj7 여부를 식별해내는 경량 헬퍼
fn parse_single_chord_name(word: &str) -> Option<(u8, bool, bool, bool)> {
    // (root_pitch, is_minor, is_7th, is_maj7)
    let trimmed = word.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut chars = trimmed.chars();
    let first = chars.next()?;

    // 루트 음정 획득 (대소문자 구분 없이 대문자로 변환하여 매칭함)
    let root = match first.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None, // 유효한 서양 음계 코드가 아님
    };

    // 올림(#)/내림(b) 변화 상태 파악
    let mut current_idx = 1;
    let mut modifier = 0i8;
    if let Some(second) = trimmed.chars().nth(current_idx) {
        if second == '#' {
            modifier = 1;
            current_idx += 1;
        } else if second.to_ascii_uppercase() == 'B' || second == '♭' {
            modifier = -1;
            current_idx += 1;
        }
    }

    let final_root = ((root as i8 + modifier + 12) % 12) as u8;

    // 접미사 부분 추출
    let suffix = &trimmed[current_idx..];

    let mut is_minor = false;
    let mut is_7th = false;
    let mut is_maj7 = false;

    if !suffix.is_empty() {
        let suffix_upper = suffix.to_uppercase();

        // 7화음 속성 감지
        if suffix_upper.contains('7') {
            is_7th = true;
        }

        // 마이너 속성 정밀 감지 (대소문자 구분을 통해 CM7과 Cm7의 오인식을 해결함)
        if suffix.starts_with('m') && !suffix.starts_with("maj") {
            is_minor = true;
        } else if suffix_upper.starts_with("MIN") {
            is_minor = true;
        } else if suffix.starts_with('M')
            && !suffix_upper.starts_with("MAJ")
            && !suffix.starts_with("M7")
        {
            is_minor = true;
        }

        // 메이저 7화음 속성 감지 (maj7, Maj7, MAJ7, M7 등)
        if suffix_upper.contains("MAJ7") || suffix.contains("M7") {
            is_maj7 = true;
        }
    }

    Some((final_root, is_minor, is_7th, is_maj7))
}

// 트랙 이름 등에서 파싱한 조성 문자열(소문자)을 midly 규격의 (sf, is_minor)로 변환
fn parse_key_str(s: &str) -> Option<(i8, bool)> {
    match s {
        "c" => Some((0, false)),
        "g" => Some((1, false)),
        "d" => Some((2, false)),
        "a" => Some((3, false)),
        "e" => Some((4, false)),
        "b" => Some((5, false)),
        "f#" => Some((6, false)),
        "gb" => Some((-6, false)),
        "c#" => Some((7, false)),
        "db" => Some((-5, false)),
        "ab" => Some((-4, false)),
        "eb" => Some((-3, false)),
        "bb" => Some((-2, false)),
        "f" => Some((-1, false)),
        "cb" => Some((-7, false)),

        "am" => Some((0, true)),
        "em" => Some((1, true)),
        "bm" => Some((2, true)),
        "f#m" => Some((3, true)),
        "gbm" => Some((-3, true)),
        "c#m" => Some((4, true)),
        "dbm" => Some((-4, true)),
        "g#m" => Some((5, true)),
        "abm" => Some((-7, true)),
        "d#m" => Some((6, true)),
        "ebm" => Some((-6, true)),
        "a#m" => Some((7, true)),
        "bbm" => Some((-5, true)),
        "fm" => Some((-4, true)),
        "cm" => Some((-3, true)),
        "gm" => Some((-2, true)),
        "dm" => Some((-1, true)),
        _ => None,
    }
}
