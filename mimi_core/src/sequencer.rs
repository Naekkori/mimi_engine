use midly::{Format, Smf, Track, TrackEventKind};

#[derive(Debug, Clone)]
pub enum MidiEngineEvent{
    //32 채널 확장 포트 번호 (0 또는 1) 0~15번 실제 채널 미디 메세지 종류
    MidiPlay{
        port:u8,
        channel: u8,
        kind: TrackEventKind<'static>
    },
    // 노래방 가사 (내장)
    SmfKaraokeText{
        text: String
    },
    // 리듬 변환 모드
    RhythmConversion {
        is_enable:bool
    }
}