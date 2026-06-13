// mimi_gd/src/lib.rs
// Godot GDExtension 바인딩 (gdext 기반)
//
// [사용 방식]
// 1. MimiEngine 노드를 씬에 추가
// 2. create() 또는 create_async()로 엔진 초기화 (사운드폰트 경로 + 샘플레이트)
// 3. 오디오 스트림 제너레이터 콜백 등에서 fill_buffer() 호출
// 4. play/pause/stop/seek 등으로 재생 제어
// 5. get_status()로 현재 상태 조회

use godot::builtin::{Dictionary, PackedFloat32Array, StringName, Variant};
use godot::classes::Node;
use godot::prelude::*;
use mimi_core::{
    AudioPlaybackContext, MimiCommand, MimiEngineHandle, PlayerState, Rhythm, create_mimi_engine,
};
use std::sync::{Arc, Mutex};

struct MimiExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MimiExtension {}

// 초기화 상태 머신
enum MimiInitState {
    Initializing,
    Ready {
        handle: MimiEngineHandle,
        context: AudioPlaybackContext,
    },
    Failed,
}

// 스레드 안전한 내부 핸들
struct MimiGdInner {
    init_state: MimiInitState,
    last_progress: f32,
    last_progress_msg: String,
}

// Godot 노드 클래스
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MimiEngine {
    base: Base<Node>,
    inner: Option<Arc<Mutex<MimiGdInner>>>,
}

#[godot_api]
impl INode for MimiEngine {
    fn init(base: Base<Node>) -> Self {
        Self { base, inner: None }
    }
}

#[godot_api]
impl MimiEngine {
    // 엔진 핸들 생성 (동기, 블로킹)
    // sf_path: 사운드폰트(.sf2) 파일 경로
    // sample_rate: 게임 엔진 오디오 샘플레이트 (ex: 44100.0, 48000.0)
    // 반환값: 성공 시 true, 실패 시 false
    #[func]
    fn create(&mut self, sf_path: GString, sample_rate: f64) -> bool {
        let path = sf_path.to_string();
        match create_mimi_engine(&path, sample_rate, |_, _| {}) {
            Ok((handle, context)) => {
                let inner = MimiGdInner {
                    init_state: MimiInitState::Ready { handle, context },
                    last_progress: 1.0,
                    last_progress_msg: "Ready".to_string(),
                };
                self.inner = Some(Arc::new(Mutex::new(inner)));
                true
            }
            Err(_) => {
                let inner = MimiGdInner {
                    init_state: MimiInitState::Failed,
                    last_progress: 0.0,
                    last_progress_msg: "Failed".to_string(),
                };
                self.inner = Some(Arc::new(Mutex::new(inner)));
                false
            }
        }
    }

    // 엔진 핸들 생성 (비동기, 논블로킹)
    // 백그라운드 스레드에서 초기화를 시작하고 즉시 반환
    // get_init_progress()로 진행 상태를 폴링
    #[func]
    fn create_async(&mut self, sf_path: GString, sample_rate: f64) {
        let path = sf_path.to_string();
        let inner = Arc::new(Mutex::new(MimiGdInner {
            init_state: MimiInitState::Initializing,
            last_progress: 0.0,
            last_progress_msg: String::new(),
        }));

        let inner_clone = Arc::clone(&inner);
        std::thread::spawn(move || {
            let result = create_mimi_engine(&path, sample_rate, |p, msg| {
                if let Ok(mut guard) = inner_clone.lock() {
                    guard.last_progress = p;
                    guard.last_progress_msg = msg.to_string();
                }
            });
            if let Ok(mut guard) = inner_clone.lock() {
                match result {
                    Ok((handle, context)) => {
                        guard.last_progress = 1.0;
                        guard.last_progress_msg = "Ready".to_string();
                        guard.init_state = MimiInitState::Ready { handle, context };
                    }
                    Err(_) => {
                        guard.last_progress = 0.0;
                        guard.last_progress_msg = "Failed".to_string();
                        guard.init_state = MimiInitState::Failed;
                    }
                }
            }
        });

        self.inner = Some(inner);
    }

    // 오디오 콜백 - 스테레오 인터리브드 f32 버퍼를 채움
    // buffer: L/R 인터리브드 f32 배열 (길이 = 프레임 수 * 2)
    // 초기화 미완료 시 무음(silence)을 채움
    #[func]
    fn fill_buffer(&self, mut buffer: PackedFloat32Array) -> PackedFloat32Array {
        let Some(inner) = self.inner.as_ref() else {
            buffer.fill(0.0);
            return buffer;
        };

        let Ok(mut guard) = inner.lock() else {
            buffer.fill(0.0);
            return buffer;
        };

        if let MimiInitState::Ready { context, .. } = &mut guard.init_state {
            context.fill_buffer(buffer.as_mut_slice());
        } else {
            buffer.fill(0.0);
        }
        buffer
    }

    // 재생 시작
    #[func]
    fn play(&self) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::Play);
        }
    }

    // 일시정지
    #[func]
    fn pause(&self) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::Pause);
        }
    }

    // 정지 및 초기화
    #[func]
    fn stop(&self) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::Stop);
        }
    }

    // MIDI 파일 로드
    // midi_data: MIDI 바이너리 데이터
    #[func]
    fn load_song(&self, midi_data: PackedByteArray) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::LoadSong(midi_data.to_vec()));
        }
    }

    // 조옮김 설정 (-15 ~ +15 반음)
    #[func]
    fn set_key(&self, key: i32) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::SetKey(key.clamp(-15, 15) as i8));
        }
    }

    // 템포 배율 설정 (0.2 ~ 5.0)
    #[func]
    fn set_tempo(&self, tempo: f32) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::SetTempo(tempo));
        }
    }

    // 마스터 볼륨 설정 (0 ~ 100)
    #[func]
    fn set_volume(&self, volume: u8) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::SetVolume(volume));
        }
    }

    // 특정 틱 위치로 이동 (Seek)
    #[func]
    fn seek(&self, tick: u32) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let _ = handle.send_command(MimiCommand::Seek(tick));
        }
    }

    // 리듬 모드 설정
    // rhythm: 0=Original, 1=Disco, 2=GoGo, 3=Dance, 4=Techno, 5=Hiphop, 6=Jitterbug, 7=Edm
    #[func]
    fn set_rhythm(&self, rhythm: i32) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };
        let Ok(guard) = inner.lock() else { return };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let r = match rhythm {
                1 => Rhythm::Disco,
                2 => Rhythm::GoGo,
                3 => Rhythm::Dance,
                4 => Rhythm::Techno,
                5 => Rhythm::Hiphop,
                6 => Rhythm::Jitterbug,
                7 => Rhythm::Edm,
                8 => Rhythm::Edm2,
                _ => Rhythm::Original,
            };
            let _ = handle.send_command(MimiCommand::SetRhythm(r));
        }
    }

    // 비동기 초기화 진행 상태 조회
    // 반환값: Dictionary { state: i32, progress: f32, message: String }
    //   state: 0=초기화 중, 1=완료, -1=실패
    #[func]
    fn get_init_progress(&self) -> Dictionary<Variant, Variant> {
        let mut dict = Dictionary::new();
        let Some(inner) = self.inner.as_ref() else {
            dict.set("state", &Variant::from(-1i64));
            dict.set("progress", &Variant::from(0.0f64));
            dict.set("message", &Variant::from(""));
            return dict;
        };
        let Ok(guard) = inner.lock() else {
            dict.set("state", &Variant::from(-1i64));
            dict.set("progress", &Variant::from(0.0f64));
            dict.set("message", &Variant::from(""));
            return dict;
        };
        let state = match &guard.init_state {
            MimiInitState::Initializing => 0i64,
            MimiInitState::Ready { .. } => 1i64,
            MimiInitState::Failed => -1i64,
        };
        dict.set("state", &Variant::from(state));
        dict.set("progress", &Variant::from(guard.last_progress as f64));
        dict.set("message", &Variant::from(guard.last_progress_msg.as_str()));
        dict
    }

    // 현재 엔진 상태 조회
    // 반환값: Dictionary { state, current_tick, total_tick, current_time_sec, tempo, key, volume, current_tempo, is_bs_detected, current_rhythm }
    //   state: 0=Stopped, 1=Playing, 2=Paused
    #[func]
    fn get_status(&self) -> Dictionary<Variant, Variant> {
        let mut dict = Dictionary::new();
        let Some(inner) = self.inner.as_ref() else {
            return dict;
        };
        let Ok(guard) = inner.lock() else { return dict };
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            if let Ok(status) = handle.get_status() {
                let state_int = match status.state {
                    PlayerState::Stopped => 0i64,
                    PlayerState::Playing => 1i64,
                    PlayerState::Paused => 2i64,
                };
                let rhythm_int = match status.current_rhythm {
                    Rhythm::Original => 0i64,
                    Rhythm::Disco => 1i64,
                    Rhythm::GoGo => 2i64,
                    Rhythm::Dance => 3i64,
                    Rhythm::Techno => 4i64,
                    Rhythm::Hiphop => 5i64,
                    Rhythm::Jitterbug => 6i64,
                    Rhythm::Edm => 7i64,
                    Rhythm::Edm2 => 8i64,
                };
                dict.set("state", &Variant::from(state_int));
                dict.set("current_tick", &Variant::from(status.current_tick as i64));
                dict.set("total_tick", &Variant::from(status.total_tick as i64));
                dict.set(
                    "current_time_sec",
                    &Variant::from(status.current_time.as_secs_f64()),
                );
                dict.set("tempo", &Variant::from(status.tempo as f64));
                dict.set("key", &Variant::from(status.key as i64));
                dict.set("volume", &Variant::from(status.volume as i64));
                dict.set("current_tempo", &Variant::from(status.current_tempo as i64));
                dict.set("is_bs_detected", &Variant::from(status.is_bs_detected));
                dict.set("current_rhythm", &Variant::from(rhythm_int));
                dict.set("ppq", &Variant::from(status.ppq as i64));
            }
        }
        dict
    }

    // 엔진 정보 조회
    // 반환값: Dictionary { name, version, author, license }
    #[func]
    fn get_engine_info(&self) -> Dictionary<Variant, Variant> {
        let mut dict = Dictionary::new();
        let info = mimi_core::get_engine_info();
        dict.set("name", &Variant::from(info.name.as_str()));
        dict.set("version", &Variant::from(info.version.as_str()));
        dict.set("author", &Variant::from(info.author.as_str()));
        dict.set("license", &Variant::from(info.license.as_str()));
        dict
    }
}
