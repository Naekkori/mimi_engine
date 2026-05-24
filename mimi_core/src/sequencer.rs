use midly::{Smf, TrackEventKind};

#[derive(Debug, Clone)]
pub enum MidiEngineEvent {
    // 단일 포트(16채널) 기준 미디 메세지 종류
    MidiPlay {
        channel: u8,
        is_drum_channel: bool, // 드럼 채널 여부 추가 (10번 또는 11번 채널)
        kind: TrackEventKind<'static>,
    },
    // 템포 변경 이벤트 분리
    TempoChange {
        tempo: u32,
    },
    // 노래방 가사 (내장)
    SmfKaraokeText {
        text: String,
    },
    //재생 진행 상태
    TickUpdate{
        current_tick: u64,
        total_tick: u64,
    }
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
        for (_track_idx, track) in smf.tracks.iter().enumerate() {
            let mut accum_tick = 0u32;

            for event in track.iter() {
                accum_tick += event.delta.as_int();
                let mut priority = 1; // 기본은 연주(Note) 우선순위

                let kind = event.kind.to_static();

                match &kind {
                    // 포트 메타 이벤트는 무시 (단일 포트로 압축)
                    TrackEventKind::Meta(midly::MetaMessage::MidiPort(_)) => {}
                    // 내장가사(Lyric) 및 일반 텍스트(Text) 이벤트 모두 처리
                    TrackEventKind::Meta(midly::MetaMessage::Lyric(bytes)) | 
                    TrackEventKind::Meta(midly::MetaMessage::Text(bytes)) => {
                        // UTF-8이 아닌 경우(CP949 등)를 고려하여 손실 허용 변환
                        let text = String::from_utf8_lossy(bytes).to_string();
                        
                        // 제어 문자나 메타데이터(예: @T, @L 등)가 아닌 실제 텍스트가 있을 때만 추가
                        if !text.is_empty() && !text.starts_with('@') {
                            priority = 0; 
                            all_events.push(SequenceEvent {
                                absolute_tick: accum_tick,
                                priority,
                                inner: MidiEngineEvent::SmfKaraokeText { text },
                            });
                        }
                    }
                    // 연주이벤트
                    TrackEventKind::Midi { channel, message } => {
                        // 표준 MIDI 규격에 따라 9번 채널(10번)만 기본 드럼으로 처리
                        let is_drum = u8::from(*channel) == 9;
                        
                        // CC나 Program Change는 Note보다 우선순위가 높아야 함
                        match message {
                            midly::MidiMessage::NoteOn { .. } | midly::MidiMessage::NoteOff { .. } => priority = 1,
                            _ => priority = 0,
                        }

                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority,
                            inner: MidiEngineEvent::MidiPlay {
                                channel: u8::from(*channel),
                                is_drum_channel: is_drum, // 드럼 채널 여부 설정
                                kind: TrackEventKind::Midi {
                                    channel: *channel,
                                    message: *message,
                                },
                            },
                        })
                    },
                    // 템포 변경 이벤트도 시퀀서가 트래킹할 수 있도록 포함
                    TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo)) => {
                        all_events.push(SequenceEvent {
                            absolute_tick: accum_tick,
                            priority: 0,
                            inner: MidiEngineEvent::TempoChange { tempo: tempo.as_int() },
                        });
                    }
                    _ => {}
                }
            }
        }
        // 절대틱 오름차순, 같은 틱이면 우선순위(priority) 오름차순으로 정렬
        all_events.sort_by(|a, b| {
            a.absolute_tick.cmp(&b.absolute_tick)
                .then(a.priority.cmp(&b.priority))
        });

        // 마지막 이벤트의 절대틱을 총 틱으로 설정.
        let total_ticks = all_events.last().map(|e| e.absolute_tick).unwrap_or(0);
        // 초기템포 설정
        let initial_per_beat = 500_000.0;
        let microseconds_per_tick = initial_per_beat / ppq as f64;

        Ok(Self {
            event: all_events,
            ppq,
            current_event_index: 0,
            current_tick: 0.0,
            microseconds_per_tick,
            total_ticks,
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
}
