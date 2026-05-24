use midly::{Smf, TrackEventKind};

#[derive(Debug, Clone)]
pub enum MidiEngineEvent {
    //32 채널 확장 포트 번호 (0 또는 1) 0~15번 실제 채널 미디 메세지 종류
    MidiPlay {
        port: u8,
        channel: u8,
        kind: TrackEventKind<'static>,
    },
    // 노래방 가사 (내장)
    SmfKaraokeText {
        text: String,
    },
    // 리듬 변환 모드
    RhythmConversion {
        is_enable: bool,
    },
}

#[derive(Debug, Clone)]
pub struct SequenceEvent {
    pub absolute_tick: u32,
    pub inner: MidiEngineEvent,
}

pub struct MimiSequencer {
    pub event: Vec<SequenceEvent>,
    pub ppq: u16,
    pub current_event_index: usize,
    pub current_tick: f64,
    pub microseconds_per_tick: f64,
}

impl MimiSequencer {
    pub fn from_byte(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        let smf = Smf::parse(bytes).map_err(|e| anyhow::anyhow!("SMF 파싱 실패: {:?}", e))?;

        let ppq = match smf.header.timing {
            midly::Timing::Metrical(ticks) => ticks.as_int(),
            _ => 480,
        };

        let mut all_events = Vec::new();

        // 멀티트랙 미디파일을 단일절대 틱 타임라인으로 병합
        for (track_idx, track) in smf.tracks.iter().enumerate() {
            let mut accum_tick = 0u32;
            let mut current_port = 0u8; // 기본포트 A(0)

            for event in track.iter() {
                accum_tick += event.delta.as_int();

                let kind = event.kind.to_static();

                match &kind {
                    TrackEventKind::Meta(midly::MetaMessage::MidiPort(port)) => {
                        current_port = u8::from(*port);
                    }
                    // 내장가사
                    TrackEventKind::Meta(midly::MetaMessage::Lyric(bytes)) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            all_events.push(SequenceEvent {
                                absolute_tick: accum_tick,
                                inner: MidiEngineEvent::SmfKaraokeText { text },
                            })
                        }
                    }
                    // 연주이벤트
                    TrackEventKind::Midi { channel, message } => all_events.push(SequenceEvent {
                        absolute_tick: accum_tick,
                        inner: MidiEngineEvent::MidiPlay {
                            port: current_port,
                            channel: u8::from(*channel),
                            kind: TrackEventKind::Midi {
                                channel: *channel,
                                message: *message,
                            },
                        },
                    }),
                    // 템포 변경 이벤트도 시퀀서가 트래킹할 수 있도록 포함
                    TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo)) => {
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            inner: MidiEngineEvent::MidiPlay {
                                port: 0,
                                channel: 0,
                                kind: TrackEventKind::Meta(midly::MetaMessage::Tempo(*tempo)),
                            },
                        });
                    }
                    _ => {}
                }
            }
        }
        // 모든 트랙 절대틱 기준 오름차순 으로
        all_events.sort_by_key(|e| e.absolute_tick);

        // 초기템포 설정
        let initial_per_beat = 500_000.0;
        let microseconds_per_tick = initial_per_beat / ppq as f64;

        Ok(Self {
            event: all_events,
            ppq,
            current_event_index: 0,
            current_tick: 0.0,
            microseconds_per_tick,
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

        //현재 틱 위치까지 도달한 이벤트 전부 가져옴
        while self.current_event_index < self.event.len() {
            let event = &self.event[self.current_event_index];
            if event.absolute_tick as f64 <= self.current_tick {
                // 도중 템포변경 이벤트 를 만나면 내부 틱당 소요시간 변경
                if let MidiEngineEvent::MidiPlay {
                    kind: TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo)),
                    ..
                } = &event.inner
                {
                    let per_beat = tempo.as_int() as f64;
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
        self.current_tick += 0.0;
        self.microseconds_per_tick = 500_000.0 / self.ppq as f64;
    }
}
