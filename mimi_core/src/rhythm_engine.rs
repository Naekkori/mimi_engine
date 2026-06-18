/// 리듬엔진
/// 알고리즘에 따른 리듬변환 구현
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rhythm {
    Disco,
    GoGo,
    Dance,
    Techno,
    Hiphop,
    Jitterbug,
    Edm,
    Edm2,
    Original, // 원곡 (꺼짐)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackType {
    Drum,          // 음정 변환 없음 (그대로 출력)
    Bass,          // 코드 근음(Root)에 맞춰 이조
    Accompaniment, // 건반, 기타 등 코드 구성음에 맞춰 이조
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MidiNote {
    pub tick: u32,
    pub note_number: u8,
    pub velocity: u8, // velocity가 0이면 Note-Off를 상징
    pub channel: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RhythmTrack {
    pub track_type: TrackType,
    pub instrument_program: u8, // GM 패치 번호
    pub notes: Vec<MidiNote>,
}

/// 멀티 트랙을 지원하는 리듬 패턴 구조체
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdvancedRhythmPattern {
    pub length_ticks: u32, // 패턴 루프의 총 길이 (예: 1마디 = 1920)
    pub tracks: Vec<RhythmTrack>,
}

/// 원곡의 소리 안 나는 $BS 트랙에서 실시간 분석해 낸 코드 이벤트
#[derive(Clone, Debug)]
pub struct BsChordEvent {
    pub tick: u32,
    pub root_pitch: u8,  // C=0, C#=1, D=2 ... B=11 (옥타브 무관 정규화된 값)
    pub is_minor: bool,  // 단순화를 위해 Major/Minor 분기 처리 (확장 가능)
    pub is_7th: bool,    // 7화음 여부
    pub is_maj7: bool,   // 메이저 7화음 여부 (C7 vs CMaj7 구분용)
}

pub struct RhythmEngine {
    pub is_enable: bool,
    pub current_rhythm: Rhythm,
    pub pattern_library: HashMap<Rhythm, AdvancedRhythmPattern>,
}

impl RhythmEngine {
    pub fn new(current_rhythm: Rhythm) -> Self {
        let mut engine = Self {
            is_enable: true,
            current_rhythm,
            pattern_library: HashMap::new(),
        };
        engine.register_default_patterns();
        engine
    }

    /// 기본 탑재할 번들 리듬 패턴(디스코, 고고, 테크노, 댄스) 초기 로드
    /// 패턴 데이터는 assets/rhythm_patterns.json 파일에서 컴파일 타임에 임베드하여 로드함
    /// 본 리듬 패턴은 기본 PPQ=480 기준으로 작성되었음
    fn register_default_patterns(&mut self) {
        // 번들된 JSON 파일을 컴파일 타임에 임베드하여 파싱
        const PATTERNS_JSON: &str = include_str!("../assets/rhythm_patterns.json");

        #[derive(Deserialize)]
        struct PatternsFile {
            patterns: HashMap<String, AdvancedRhythmPattern>,
        }

        // JSON 키 문자열을 Rhythm enum으로 매핑하는 내부 헬퍼
        fn parse_rhythm_name(name: &str) -> Option<Rhythm> {
            match name {
                "Disco" => Some(Rhythm::Disco),
                "GoGo" => Some(Rhythm::GoGo),
                "Dance" => Some(Rhythm::Dance),
                "Techno" => Some(Rhythm::Techno),
                "Hiphop" => Some(Rhythm::Hiphop),
                "Jitterbug" => Some(Rhythm::Jitterbug),
                "Edm" => Some(Rhythm::Edm),
                "Edm2" => Some(Rhythm::Edm2),
                "Original" => Some(Rhythm::Original),
                _ => None,
            }
        }

        let parsed: PatternsFile = serde_json::from_str(PATTERNS_JSON)
            .expect("rhythm_patterns.json 파싱 실패");

        for (name, pattern) in parsed.patterns {
            if let Some(rhythm) = parse_rhythm_name(&name) {
                self.pattern_library.insert(rhythm, pattern);
            }
        }
    }

    /// 원곡의 총 길이와 $BS 트랙 이벤트 배열을 받아 전체 변환 세션 트랙을 생성하는 핵심 함수
    /// ppq_scale: 원곡의 PPQ가 기본 480이 아닐 경우(예: 96, 960 등) 패턴 재생 속도를 맞추기 위해 스케일링
    pub fn generate_accompaniment_tracks(
        &self,
        total_duration_ticks: u32,
        bs_timeline: &[BsChordEvent],
        source_ppq: u16,
    ) -> Vec<MidiNote> {
        let mut generated_notes = Vec::new();

        if !self.is_enable || bs_timeline.is_empty() {
            return generated_notes;
        }

        let ppq_ratio = source_ppq as f64 / 480.0;

        // 1. 선택된 리듬 패턴 세트 로드
        if let Some(pattern) = self.pattern_library.get(&self.current_rhythm) {
            let scaled_length_ticks = (pattern.length_ticks as f64 * ppq_ratio).round() as u32;

            let mut current_offset = 0;

            // 각 트랙의 각 음표 번호별로 마지막에 적용된 이조 값을 저장 (Note-Off 시 동일 이조 적용을 위함)
            let mut last_transpositions: HashMap<(usize, u8), u8> = HashMap::new();

            // 2. 원곡 길이만큼 리듬 루프 생성 진입
            while current_offset < total_duration_ticks {
                let is_last_measure = current_offset + scaled_length_ticks >= total_duration_ticks;

                // 3. 루프 내의 각 악기 트랙별로 처리
                for (track_idx, track) in pattern.tracks.iter().enumerate() {
                    if is_last_measure {
                        // 곡이 끝나는 마지막 마디: 필인(스네어 롤) 및 엔딩 섹션(드럼 크래시 및 악기 엔딩 화음) 적용
                        match track.track_type {
                            TrackType::Drum => {
                                // 1. 원래 드럼 패턴 중 960틱 이전(전반부)의 노트만 필터링하여 재생
                                for note in &track.notes {
                                    if note.tick < 960 {
                                        let scaled_note_tick = (note.tick as f64 * ppq_ratio).round() as u32;
                                        let target_tick = current_offset + scaled_note_tick;
                                        if target_tick < total_duration_ticks {
                                            generated_notes.push(MidiNote {
                                                tick: target_tick,
                                                note_number: note.note_number,
                                                velocity: note.velocity,
                                                channel: note.channel,
                                            });
                                        }
                                    }
                                }

                                // 2. 후반부 3박/4박 구간(960틱부터 1680틱 미만) 스네어 롤 필인 적용 (점진적 빌드업)
                                let fill_in_ticks = [960, 1080, 1200, 1320, 1440, 1500, 1560, 1620];
                                for (i, &f_tick) in fill_in_ticks.iter().enumerate() {
                                    let scaled_f_tick = (f_tick as f64 * ppq_ratio).round() as u32;
                                    let target_tick = current_offset + scaled_f_tick;
                                    if target_tick < total_duration_ticks {
                                        let vel = 70 + (i * 5) as u8; // 점진적 강화 (70 -> 105)
                                        generated_notes.push(MidiNote {
                                            tick: target_tick,
                                            note_number: 38, // GM Standard Snare
                                            velocity: vel,
                                            channel: 9,
                                        });
                                        generated_notes.push(MidiNote {
                                            tick: target_tick + 80,
                                            note_number: 38,
                                            velocity: 0,
                                            channel: 9,
                                        });
                                    }
                                }

                                // 3. 최종 엔딩 쾅 (1680틱) - 강한 킥(36) + 크래시 심벌(49) 동시 타격
                                let ending_tick = (1680.0 * ppq_ratio).round() as u32;
                                let target_tick = current_offset + ending_tick;
                                if target_tick < total_duration_ticks {
                                    // 킥 타격
                                    generated_notes.push(MidiNote {
                                        tick: target_tick,
                                        note_number: 36,
                                        velocity: 127,
                                        channel: 9,
                                    });
                                    generated_notes.push(MidiNote {
                                        tick: target_tick + 200,
                                        note_number: 36,
                                        velocity: 0,
                                        channel: 9,
                                    });
                                    // 크래시 심벌 타격
                                    generated_notes.push(MidiNote {
                                        tick: target_tick,
                                        note_number: 49,
                                        velocity: 127,
                                        channel: 9,
                                    });
                                    generated_notes.push(MidiNote {
                                        tick: target_tick + 200,
                                        note_number: 49,
                                        velocity: 0,
                                        channel: 9,
                                    });
                                }
                            }
                            TrackType::Bass => {
                                // 베이스 엔딩: 마지막 마디 시작부(0틱) 근음 길게 타건 후, 필인 진입 전(960틱) 깔끔하게 뮤트
                                // 그리고 드럼 엔딩 쾅(1680틱) 타이밍에 강하게 근음 한 번 더 짚어주고 종료
                                let current_chord = match self.get_chord_at_tick(current_offset, bs_timeline) {
                                    Some(chord) => chord,
                                    None => &bs_timeline[0],
                                };

                                for note in &track.notes {
                                    if note.tick == 0 && note.velocity > 0 {
                                        let shifted = (note.note_number as i16 + current_chord.root_pitch as i16).clamp(0, 127) as u8;
                                        
                                        // 0틱 타건 시작
                                        generated_notes.push(MidiNote {
                                            tick: current_offset,
                                            note_number: shifted,
                                            velocity: 110,
                                            channel: note.channel,
                                        });
                                        // 960틱 뮤트
                                        let mute_tick = (960.0 * ppq_ratio).round() as u32;
                                        generated_notes.push(MidiNote {
                                            tick: current_offset + mute_tick,
                                            note_number: shifted,
                                            velocity: 0,
                                            channel: note.channel,
                                        });

                                        // 1680틱 최종 강타 엔딩
                                        let ending_tick = (1680.0 * ppq_ratio).round() as u32;
                                        let target_ending = current_offset + ending_tick;
                                        if target_ending < total_duration_ticks {
                                            generated_notes.push(MidiNote {
                                                tick: target_ending,
                                                note_number: shifted,
                                                velocity: 120,
                                                channel: note.channel,
                                            });
                                            generated_notes.push(MidiNote {
                                                tick: target_ending + 200,
                                                note_number: shifted,
                                                velocity: 0,
                                                channel: note.channel,
                                            });
                                        }
                                        break;
                                    }
                                }
                            }
                            TrackType::Accompaniment => {
                                // 반주 엔딩: 0틱에 코드 화음 타건해 960틱까지 유지하고 뮤트
                                // 1680틱 드럼 엔딩 쾅 타이밍에 강하게 코드 화음 한 번 동시 타건하고 완전 종료
                                let current_chord = match self.get_chord_at_tick(current_offset, bs_timeline) {
                                    Some(chord) => chord,
                                    None => &bs_timeline[0],
                                };

                                for note in &track.notes {
                                    if note.tick == 0 && note.velocity > 0 {
                                        let mut shifted = note.note_number as i16 + current_chord.root_pitch as i16;
                                        if current_chord.is_minor && (note.note_number % 12 == 4) {
                                            shifted -= 1;
                                        }
                                        // 7화음 보정 논리 추가 반영
                                        if current_chord.is_7th && !current_chord.is_maj7 && (note.note_number % 12 == 11) {
                                            shifted -= 1;
                                        }
                                        let final_note = shifted.clamp(0, 127) as u8;

                                        // 0틱 시작
                                        generated_notes.push(MidiNote {
                                            tick: current_offset,
                                            note_number: final_note,
                                            velocity: 95,
                                            channel: note.channel,
                                        });
                                        // 960틱 뮤트
                                        let mute_tick = (960.0 * ppq_ratio).round() as u32;
                                        generated_notes.push(MidiNote {
                                            tick: current_offset + mute_tick,
                                            note_number: final_note,
                                            velocity: 0,
                                            channel: note.channel,
                                        });

                                        // 1680틱 최종 엔딩 쾅
                                        let ending_tick = (1680.0 * ppq_ratio).round() as u32;
                                        let target_ending = current_offset + ending_tick;
                                        if target_ending < total_duration_ticks {
                                            generated_notes.push(MidiNote {
                                                tick: target_ending,
                                                note_number: final_note,
                                                velocity: 115,
                                                channel: note.channel,
                                            });
                                            generated_notes.push(MidiNote {
                                                tick: target_ending + 200,
                                                note_number: final_note,
                                                velocity: 0,
                                                channel: note.channel,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // 기존 리듬 패턴 연주 (일반 마디)
                        for note in &track.notes {
                            let scaled_note_tick = (note.tick as f64 * ppq_ratio).round() as u32;
                            let target_tick = current_offset + scaled_note_tick;

                            if target_tick >= total_duration_ticks {
                                break;
                            }

                            // 4. 이 노트가 위치할 타임라인에서 가장 최신의 $BS 코드 구하기
                            let current_chord = match self.get_chord_at_tick(target_tick, bs_timeline) {
                                Some(chord) => chord,
                                None => &bs_timeline[0], // 못 찾으면 첫 번째 코드 적용
                            };

                            // 5. 악기 성격에 따른 트랜스포즈 알고리즘 수행
                            let final_note_number;

                            if note.velocity > 0 {
                                // Note-On: 현재 코드에 맞춰 이조 수행
                                let mut shifted = note.note_number as i16 + current_chord.root_pitch as i16;

                                match track.track_type {
                                    TrackType::Drum => {
                                        // 드럼 채널은 음정 변환 없이 통과
                                        final_note_number = note.note_number;
                                    }
                                    TrackType::Bass | TrackType::Accompaniment => {
                                        // 패턴 리듬 파일이 C-Major(C=0) 기준으로 제작되었다고 가정
                                        // 만약 원곡 코드가 Minor인데, 패턴 소스가 장3도(E 성분)를 연주 중이라면 단3도로 보정함
                                        if current_chord.is_minor && (note.note_number % 12 == 4) {
                                            shifted -= 1; // 반음 내림
                                        }
                                        // 원곡 코드가 7화음이고 메이저 7th가 아니면(도미넌트7th, 마이너7th) 장7도(B, 11)를 단7도로 보정함
                                        if current_chord.is_7th && !current_chord.is_maj7 && (note.note_number % 12 == 11) {
                                            shifted -= 1; // 반음 내림
                                        }
                                        final_note_number = shifted.clamp(0, 127) as u8;
                                    }
                                }
                                // 이조된 값을 기록해 둠
                                last_transpositions.insert((track_idx, note.note_number), final_note_number);
                            } else {
                                // Note-Off: 이전에 해당 음표에 적용했던 이조 값을 그대로 사용
                                // 기록이 없으면(이론상 불가능하지만 방어적 코드) 기본값 사용
                                final_note_number = *last_transpositions.get(&(track_idx, note.note_number)).unwrap_or(&note.note_number);
                            }

                            generated_notes.push(MidiNote {
                                tick: target_tick,
                                note_number: final_note_number,
                                velocity: note.velocity,
                                channel: note.channel,
                            });
                        }
                    }
                }
                // 다음 마디 오프셋 이동
                current_offset += scaled_length_ticks;
            }
        }

        // 시퀀싱을 위해 생성된 노트를 틱 순서대로 정렬하되, 
        // 같은 틱일 경우 Note-Off(velocity=0)가 Note-On(velocity>0)보다 먼저 오도록 함
        generated_notes.sort_by(|a, b| {
            a.tick.cmp(&b.tick).then(a.velocity.cmp(&b.velocity))
        });
        generated_notes
    }

    /// 특정 재생 시점(Tick)에 매핑되는 $BS 코드를 타임라인에서 이진 탐색으로 효율적으로 찾는 헬퍼
    pub fn get_chord_at_tick<'a>(&self, tick: u32, timeline: &'a [BsChordEvent]) -> Option<&'a BsChordEvent> {
        if timeline.is_empty() { return None; }
        
        // binary_search_by를 이용하여 현재 틱보다 작거나 같은 시점의 마지막 이벤트를 스캔
        match timeline.binary_search_by(|probe| probe.tick.cmp(&tick)) {
            Ok(index) => Some(&timeline[index]),
            Err(index) => {
                if index == 0 {
                    None
                } else {
                    Some(&timeline[index - 1])
                }
            }
        }
    }
}