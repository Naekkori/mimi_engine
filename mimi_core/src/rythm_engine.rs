/// 리듬엔진
/// 알고리즘에 따른 리듬변환 구현
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rhythm {
    Disco,
    GoGo,
    Dance,
    Techno,
    Hiphop,
    Jitterbug,
    Original, // 원곡 (꺼짐)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackType {
    Drum,          // 음정 변환 없음 (그대로 출력)
    Bass,          // 코드 근음(Root)에 맞춰 이조
    Accompaniment, // 건반, 기타 등 코드 구성음에 맞춰 이조
}

#[derive(Clone, Debug)]
pub struct MidiNote {
    pub tick: u32,
    pub note_number: u8,
    pub velocity: u8, // velocity가 0이면 Note-Off를 상징
    pub channel: u8,
}

#[derive(Clone, Debug)]
pub struct RhythmTrack {
    pub track_type: TrackType,
    pub instrument_program: u8, // GM 패치 번호
    pub notes: Vec<MidiNote>,
}

/// 멀티 트랙을 지원하는 리듬 패턴 구조체
#[derive(Clone, Debug)]
pub struct AdvancedRhythmPattern {
    pub length_ticks: u32, // 패턴 루프의 총 길이 (예: 1마디 = 1920)
    pub tracks: Vec<RhythmTrack>,
}

/// 원곡의 소리 안 나는 $BS 트랙에서 실시간 분석해 낸 코드 이벤트
#[derive(Clone, Debug)]
pub struct BsChordEvent {
    pub tick: u32,
    pub root_pitch: u8, // C=0, C#=1, D=2 ... B=11 (옥타브 무관 정규화된 값)
    pub is_minor: bool, // 단순화를 위해 Major/Minor 분기 처리 (확장 가능)
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
    /// 본 리듬 패턴은 기본 PPQ=480 기준으로 작성되었음
    fn register_default_patterns(&mut self) { 
        // 1. 디스코 패턴 (1마디 = 1920틱)
        let mut disco_tracks = Vec::new();

        // 디스코 드럼
        let mut disco_drum_notes = Vec::new();
        // 정박 4비트 킥 드럼 (36)
        for tick in &[0, 480, 960, 1440] {
            disco_drum_notes.push(MidiNote { tick: *tick, note_number: 36, velocity: 110, channel: 9 });
            disco_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 36, velocity: 0, channel: 9 });
        }
        // 백비트 스네어 (38)
        for tick in &[480, 1440] {
            disco_drum_notes.push(MidiNote { tick: *tick, note_number: 38, velocity: 105, channel: 9 });
            disco_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 38, velocity: 0, channel: 9 });
        }
        // 엇박 오픈하이햇 (46)
        for tick in &[240, 720, 1200, 1680] {
            disco_drum_notes.push(MidiNote { tick: *tick, note_number: 46, velocity: 90, channel: 9 });
            disco_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 46, velocity: 0, channel: 9 });
        }
        // 정박 클로즈하이햇 (42)
        for tick in &[0, 480, 960, 1440] {
            disco_drum_notes.push(MidiNote { tick: *tick, note_number: 42, velocity: 85, channel: 9 });
            disco_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 42, velocity: 0, channel: 9 });
        }
        disco_tracks.push(RhythmTrack {
            track_type: TrackType::Drum,
            instrument_program: 0, // 드럼 키트
            notes: disco_drum_notes,
        });

        // 디스코 베이스 (8비트 옥타브 교대 질주 - 뚝뚝 끊기도록 Note-Off 페어링)
        let mut disco_bass_notes = Vec::new();
        let base_roots = [
            (0, 36), (240, 48), (480, 36), (720, 48),
            (960, 36), (1200, 48), (1440, 36), (1680, 48)
        ];
        for &(tick, note) in &base_roots {
            disco_bass_notes.push(MidiNote { tick, note_number: note, velocity: 100, channel: 1 });
            disco_bass_notes.push(MidiNote { tick: tick + 160, note_number: note, velocity: 0, channel: 1 });
        }
        disco_tracks.push(RhythmTrack {
            track_type: TrackType::Bass,
            instrument_program: 33, // Electric Bass (finger)
            notes: disco_bass_notes,
        });

        // 디스코 피아노 반주 (엇박 스타카토 느낌으로 짧게 끊어줌)
        let mut disco_piano_notes = Vec::new();
        for &tick in &[240, 720, 1200, 1680] {
            disco_piano_notes.push(MidiNote { tick, note_number: 60, velocity: 85, channel: 2 }); // C
            disco_piano_notes.push(MidiNote { tick, note_number: 64, velocity: 85, channel: 2 }); // E
            disco_piano_notes.push(MidiNote { tick, note_number: 67, velocity: 85, channel: 2 }); // G

            disco_piano_notes.push(MidiNote { tick: tick + 120, note_number: 60, velocity: 0, channel: 2 }); 
            disco_piano_notes.push(MidiNote { tick: tick + 120, note_number: 64, velocity: 0, channel: 2 }); 
            disco_piano_notes.push(MidiNote { tick: tick + 120, note_number: 67, velocity: 0, channel: 2 }); 
        }
        disco_tracks.push(RhythmTrack {
            track_type: TrackType::Accompaniment,
            instrument_program: 0, // Acoustic Grand Piano
            notes: disco_piano_notes,
        });

        self.pattern_library.insert(Rhythm::Disco, AdvancedRhythmPattern {
            length_ticks: 1920,
            tracks: disco_tracks,
        });

        // 2. 고고 패턴 (싱코페이션이 스며든 올드스쿨 리듬)
        let mut gogo_tracks = Vec::new();

        // 고고 드럼
        let mut gogo_drum_notes = Vec::new();
        // 킥 드럼 변형 형태
        for tick in &[0, 720, 960, 1200] {
            gogo_drum_notes.push(MidiNote { tick: *tick, note_number: 36, velocity: 110, channel: 9 });
            gogo_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 36, velocity: 0, channel: 9 });
        }
        // 스네어 (백비트)
        for tick in &[480, 1440] {
            gogo_drum_notes.push(MidiNote { tick: *tick, note_number: 38, velocity: 105, channel: 9 });
            gogo_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 38, velocity: 0, channel: 9 });
        }
        // 8비트 고정 하이햇 (42)
        for tick in &[0, 240, 480, 720, 960, 1200, 1440, 1680] {
            gogo_drum_notes.push(MidiNote { tick: *tick, note_number: 42, velocity: 80, channel: 9 });
            gogo_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 42, velocity: 0, channel: 9 });
        }
        gogo_tracks.push(RhythmTrack {
            track_type: TrackType::Drum,
            instrument_program: 0,
            notes: gogo_drum_notes,
        });

        // 고고 베이스 (워킹형 바운스)
        let mut gogo_bass_notes = Vec::new();
        let gogo_bass = [
            (0, 36), (480, 40), (720, 43),
            (960, 36), (1440, 40), (1680, 43)
        ];
        for &(tick, note) in &gogo_bass {
            gogo_bass_notes.push(MidiNote { tick, note_number: note, velocity: 100, channel: 1 });
            gogo_bass_notes.push(MidiNote { tick: tick + 160, note_number: note, velocity: 0, channel: 1 });
        }
        gogo_tracks.push(RhythmTrack {
            track_type: TrackType::Bass,
            instrument_program: 33,
            notes: gogo_bass_notes,
        });

        // 고고 피아노 반주 (4비트 코드로 기둥 연주)
        let mut gogo_piano_notes = Vec::new();
        for &tick in &[0, 480, 960, 1440] {
            gogo_piano_notes.push(MidiNote { tick, note_number: 60, velocity: 85, channel: 2 });
            gogo_piano_notes.push(MidiNote { tick, note_number: 64, velocity: 85, channel: 2 });
            gogo_piano_notes.push(MidiNote { tick, note_number: 67, velocity: 85, channel: 2 });

            gogo_piano_notes.push(MidiNote { tick: tick + 200, note_number: 60, velocity: 0, channel: 2 });
            gogo_piano_notes.push(MidiNote { tick: tick + 200, note_number: 64, velocity: 0, channel: 2 });
            gogo_piano_notes.push(MidiNote { tick: tick + 200, note_number: 67, velocity: 0, channel: 2 });
        }
        gogo_tracks.push(RhythmTrack {
            track_type: TrackType::Accompaniment,
            instrument_program: 0,
            notes: gogo_piano_notes,
        });

        self.pattern_library.insert(Rhythm::GoGo, AdvancedRhythmPattern {
            length_ticks: 1920,
            tracks: gogo_tracks,
        });

        // 3. 테크노 패턴 (초고속 질주 비트)
        let mut techno_tracks = Vec::new();
        let mut techno_drum_notes = Vec::new();
        // 테크노 쿵쿵쿵쿵 사중 포화
        for tick in &[0, 480, 960, 1440] {
            techno_drum_notes.push(MidiNote { tick: *tick, note_number: 36, velocity: 120, channel: 9 });
            techno_drum_notes.push(MidiNote { tick: *tick + 80, note_number: 36, velocity: 0, channel: 9 });
            techno_drum_notes.push(MidiNote { tick: *tick + 240, note_number: 42, velocity: 90, channel: 9 });
            techno_drum_notes.push(MidiNote { tick: *tick + 320, note_number: 42, velocity: 0, channel: 9 });
        }
        // 미친 백 비트 스네어
        for tick in &[480, 1440] {
            techno_drum_notes.push(MidiNote { tick: *tick, note_number: 40, velocity: 110, channel: 9 });
            techno_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 40, velocity: 0, channel: 9 });
        }
        techno_tracks.push(RhythmTrack {
            track_type: TrackType::Drum,
            instrument_program: 0,
            notes: techno_drum_notes,
        });

        // 테크노 베이스 (16비트 연사 톱니파 라인)
        let mut techno_bass_notes = Vec::new();
        for i in 0..16 {
            let note = if i % 2 == 0 { 36 } else { 36 + 12 };
            let tick = i * 120;
            techno_bass_notes.push(MidiNote {
                tick,
                note_number: note,
                velocity: 95,
                channel: 1,
            });
            techno_bass_notes.push(MidiNote {
                tick: tick + 80,
                note_number: note,
                velocity: 0,
                channel: 1,
            });
        }
        techno_tracks.push(RhythmTrack {
            track_type: TrackType::Bass,
            instrument_program: 38, // Synth Bass 1
            notes: techno_bass_notes,
        });

        self.pattern_library.insert(Rhythm::Techno, AdvancedRhythmPattern {
            length_ticks: 1920,
            tracks: techno_tracks,
        });

        // 4. 댄스 패턴 (현대적인 클럽 하우스 풍 리듬)
        let mut dance_tracks = Vec::new();
        let mut dance_drum_notes = Vec::new();
        for tick in &[0, 480, 960, 1440] {
            dance_drum_notes.push(MidiNote { tick: *tick, note_number: 36, velocity: 115, channel: 9 });
            dance_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 36, velocity: 0, channel: 9 });
        }
        for tick in &[480, 1440] {
            dance_drum_notes.push(MidiNote { tick: *tick, note_number: 38, velocity: 110, channel: 9 });
            dance_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 38, velocity: 0, channel: 9 });
        }
        for tick in &[240, 720, 1200, 1680] {
            dance_drum_notes.push(MidiNote { tick: *tick, note_number: 46, velocity: 95, channel: 9 });
            dance_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 46, velocity: 0, channel: 9 });
        }
        dance_tracks.push(RhythmTrack {
            track_type: TrackType::Drum,
            instrument_program: 0,
            notes: dance_drum_notes,
        });

        let mut dance_bass_notes = Vec::new();
        for i in 0..8 {
            let tick = i * 240;
            let note = if i % 4 == 3 { 39 } else { 36 };
            dance_bass_notes.push(MidiNote {
                tick,
                note_number: note,
                velocity: 100,
                channel: 1,
            });
            dance_bass_notes.push(MidiNote {
                tick: tick + 120, // 길이를 더 짧게 (140 -> 120)
                note_number: note,
                velocity: 0,
                channel: 1,
            });
        }
        dance_tracks.push(RhythmTrack {
            track_type: TrackType::Bass,
            instrument_program: 39, // Synth Bass 2
            notes: dance_bass_notes,
        });

        // 댄스 반주 추가 (클럽 신스 사운드)
        let mut dance_piano_notes = Vec::new();
        for &tick in &[240, 720, 1200, 1680] {
            // C, E, G (C-Major)
            for &n in &[60, 64, 67] {
                dance_piano_notes.push(MidiNote { tick, note_number: n, velocity: 80, channel: 2 });
                dance_piano_notes.push(MidiNote { tick: tick + 100, note_number: n, velocity: 0, channel: 2 });
            }
        }
        dance_tracks.push(RhythmTrack {
            track_type: TrackType::Accompaniment,
            instrument_program: 81, // Sawtooth Lead
            notes: dance_piano_notes,
        });

        self.pattern_library.insert(Rhythm::Dance, AdvancedRhythmPattern {
            length_ticks: 1920,
            tracks: dance_tracks,
        });

        // 5. 힙합 패턴 (두툼한 90년대 붐벱 다운비트 그루브)
        let mut hiphop_tracks = Vec::new();
        let mut hiphop_drum_notes = Vec::new();
        // 킥 드럼 (36) 싱코페이션 바운스
        let hh_kick = [0, 240, 1080];
        for tick in &hh_kick {
            hiphop_drum_notes.push(MidiNote { tick: *tick, note_number: 36, velocity: 115, channel: 9 });
            hiphop_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 36, velocity: 0, channel: 9 }); // 120 -> 100
        }
        // 스네어 (38 또는 40)
        let hh_snare = [480, 1440];
        for tick in &hh_snare {
            hiphop_drum_notes.push(MidiNote { tick: *tick, note_number: 40, velocity: 110, channel: 9 });
            hiphop_drum_notes.push(MidiNote { tick: *tick + 100, note_number: 40, velocity: 0, channel: 9 }); // 120 -> 100
        }
        // 레이드백 하이햇 (42)
        for i in 0..8 {
            let tick = i * 240;
            // 엇박 오픈하이햇 포인트(오버레이)
            let pitch = if i % 4 == 2 { 46 } else { 42 };
            hiphop_drum_notes.push(MidiNote { tick, note_number: pitch, velocity: 85, channel: 9 });
            hiphop_drum_notes.push(MidiNote { tick: tick + 60, note_number: pitch, velocity: 0, channel: 9 }); // 80 -> 60
        }
        hiphop_tracks.push(RhythmTrack {
            track_type: TrackType::Drum,
            instrument_program: 24, // 힙합/R&B 키트
            notes: hiphop_drum_notes,
        });

        // 힙합 베이스 (재즈 스타일 무거운 핑거드 베이스)
        let mut hiphop_bass_notes = Vec::new();
        let hh_bass = [
            (0, 36), (480, 43), (720, 41), (1080, 36)
        ];
        for &(tick, note) in &hh_bass {
            hiphop_bass_notes.push(MidiNote { tick, note_number: note, velocity: 100, channel: 1 });
            hiphop_bass_notes.push(MidiNote { tick: tick + 150, note_number: note, velocity: 0, channel: 1 }); // 180 -> 150
        }
        hiphop_tracks.push(RhythmTrack {
            track_type: TrackType::Bass,
            instrument_program: 32, // Acoustic Bass
            notes: hiphop_bass_notes,
        });

        // 힙합 반주 (재즈 일렉트릭 피아노 코드 백킹)
        let mut hiphop_piano_notes = Vec::new();
        let hh_chords = [
            (0, vec![60, 64, 67, 71]),     // CMaj7 (C-Major 기준으로 수정)
            (960, vec![62, 65, 69, 72])    // Dm7
        ];
        for &(tick, ref notes) in &hh_chords {
            for &note in notes {
                hiphop_piano_notes.push(MidiNote { tick, note_number: note, velocity: 75, channel: 2 });
                hiphop_piano_notes.push(MidiNote { tick: tick + 400, note_number: note, velocity: 0, channel: 2 }); // 600 -> 400
            }
        }
        hiphop_tracks.push(RhythmTrack {
            track_type: TrackType::Accompaniment,
            instrument_program: 4, // Rhodes Piano
            notes: hiphop_piano_notes,
        });

        self.pattern_library.insert(Rhythm::Hiphop, AdvancedRhythmPattern {
            length_ticks: 1920,
            tracks: hiphop_tracks,
        });

        // 6. 지루박 패턴 (동네 성인 캬바레 스타일의 2비트 셔플 그루브)
        let mut jitterbug_tracks = Vec::new();
        let mut jitterbug_drum_notes = Vec::new();
        // 쿵-짝 쿵-짝 (쿵쿵 포화 킥과 강렬한 림샷)
        for i in 0..4 {
            let tick = i * 480;
            // 쿵 (킥)
            jitterbug_drum_notes.push(MidiNote { tick, note_number: 36, velocity: 120, channel: 9 });
            jitterbug_drum_notes.push(MidiNote { tick: tick + 100, note_number: 36, velocity: 0, channel: 9 }); // 120 -> 100
            // 짝 (스네어 림샷 37)
            jitterbug_drum_notes.push(MidiNote { tick: tick + 240, note_number: 37, velocity: 110, channel: 9 });
            jitterbug_drum_notes.push(MidiNote { tick: tick + 340, note_number: 37, velocity: 0, channel: 9 }); // 360 -> 340
            // 하이햇 셔플 (치키치키 소리 유도)
            jitterbug_drum_notes.push(MidiNote { tick: tick + 180, note_number: 42, velocity: 85, channel: 9 });
            jitterbug_drum_notes.push(MidiNote { tick: tick + 220, note_number: 42, velocity: 0, channel: 9 }); // 240 -> 220
            jitterbug_drum_notes.push(MidiNote { tick: tick + 420, note_number: 42, velocity: 85, channel: 9 });
            jitterbug_drum_notes.push(MidiNote { tick: tick + 460, note_number: 42, velocity: 0, channel: 9 }); // 480 -> 460
        }
        jitterbug_tracks.push(RhythmTrack {
            track_type: TrackType::Drum,
            instrument_program: 0,
            notes: jitterbug_drum_notes,
        });

        // 지루박 베이스 (단순 강렬한 튕김 비트)
        let mut jitterbug_bass_notes = Vec::new();
        for i in 0..4 {
            let tick = i * 480;
            let note = if i % 2 == 0 { 36 } else { 43 };
            jitterbug_bass_notes.push(MidiNote { tick, note_number: note, velocity: 115, channel: 1 });
            jitterbug_bass_notes.push(MidiNote { tick: tick + 150, note_number: note, velocity: 0, channel: 1 }); // 200 -> 150
            
            jitterbug_bass_notes.push(MidiNote { tick: tick + 240, note_number: note, velocity: 100, channel: 1 });
            jitterbug_bass_notes.push(MidiNote { tick: tick + 390, note_number: note, velocity: 0, channel: 1 }); // 440 -> 390
        }
        jitterbug_tracks.push(RhythmTrack {
            track_type: TrackType::Bass,
            instrument_program: 35, // Picked Bass
            notes: jitterbug_bass_notes,
        });

        // 지루박 아코디언/기타 반주 (뽕짝 특유의 엇박 칼 타건)
        let mut jitterbug_piano_notes = Vec::new();
        for i in 0..4 {
            let tick = i * 480 + 240; // 엇박 '짝'에 타건
            jitterbug_piano_notes.push(MidiNote { tick, note_number: 60, velocity: 90, channel: 2 });
            jitterbug_piano_notes.push(MidiNote { tick, note_number: 64, velocity: 90, channel: 2 });
            jitterbug_piano_notes.push(MidiNote { tick, note_number: 67, velocity: 90, channel: 2 });

            jitterbug_piano_notes.push(MidiNote { tick: tick + 120, note_number: 60, velocity: 0, channel: 2 }); // 160 -> 120
            jitterbug_piano_notes.push(MidiNote { tick: tick + 120, note_number: 64, velocity: 0, channel: 2 }); 
            jitterbug_piano_notes.push(MidiNote { tick: tick + 120, note_number: 67, velocity: 0, channel: 2 }); 
        }
        jitterbug_tracks.push(RhythmTrack {
            track_type: TrackType::Accompaniment,
            instrument_program: 21, // Accordion
            notes: jitterbug_piano_notes,
        });

        self.pattern_library.insert(Rhythm::Jitterbug, AdvancedRhythmPattern {
            length_ticks: 1920,
            tracks: jitterbug_tracks,
        });
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
                
                // 3. 루프 내의 각 악기 트랙별로 처리
                for (track_idx, track) in pattern.tracks.iter().enumerate() {
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
                                    // 만약 원곡 코드가 Minor인데, 패턴 소스가 장3도(E 성분)를 연주 중이라면 단3도로 보정
                                    if current_chord.is_minor && (note.note_number % 12 == 4) {
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
    fn get_chord_at_tick<'a>(&self, tick: u32, timeline: &'a [BsChordEvent]) -> Option<&'a BsChordEvent> {
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