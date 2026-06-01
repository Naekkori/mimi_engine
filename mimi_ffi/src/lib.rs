// mimi_ffi/src/lib.rs
// Unreal Engine / Unity / Godot 등 외부 게임 엔진을 위한 C ABI 바인딩
//
// [사용 방식]
// 1. mimi_ffi_create()로 핸들 생성 (samplerate는 게임 엔진에서 알려준 값 사용)
// 2. 게임 엔진의 오디오 콜백 안에서 mimi_ffi_fill_buffer() 호출
// 3. mimi_ffi_send_command_*() 함수로 재생 제어
// 4. mimi_ffi_destroy()로 핸들 해제

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float, c_int, c_uchar, c_uint};
use std::sync::{Arc, Mutex};
use std::thread;
use mimi_core::{
    AudioPlaybackContext, MimiCommand, MimiEngineHandle,
    PlayerState, Rhythm, create_mimi_engine,
};

// 초기화 내부 상태
enum MimiInitState {
    Initializing,
    Ready { handle: MimiEngineHandle, context: AudioPlaybackContext },
    Failed,
}

// 스레드 안전한 내부 핸들
struct MimiFfiInner {
    init_state: MimiInitState,
    // 비동기 초기화 중 프로그레스 수신용 (수동 폴링)
    last_progress: f32,
    last_progress_msg: CString,
}

// 외부에서 불투명 포인터로 다루는 핸들 구조체
pub struct MimiFfiHandle {
    inner: Arc<Mutex<MimiFfiInner>>,
}
// 엔진정보
#[repr(C)]
pub struct MimiFfiEngineInfo {
    pub name: *const c_char,
    pub version: *const c_char,
    pub author: *const c_char,
    pub license: *const c_char,
}
// FFI 경계를 통해 반환할 C 호환 상태 구조체
#[repr(C)]
pub struct MimiFfiStatus {
    // 현재 재생 상태 (0=Stopped, 1=Playing, 2=Paused)
    pub state: c_int,
    pub current_tick: c_uint,
    pub total_tick: c_uint,
    // 경과 시간 (초 단위 실수)
    pub current_time_sec: c_float,
    pub tempo: c_float,
    pub key: c_int,
    pub volume: c_uchar,
    // 현재 MIDI 파일 원본 템포 (µs/beat)
    pub current_tempo: c_int,
    // $BS 베이스 트랙 검출 여부 (0/1)
    pub is_bs_detected: c_int,
    // 현재 리듬 모드 (0=Original, 1=Disco, 2=GoGo, 3=Dance, 4=Techno, 5=Hiphop, 6=Jitterbug, 7=Edm)
    pub current_rhythm: c_int,
}

/// 엔진 핸들 생성 (동기, 블로킹)
/// sf_path: 사운드폰트(.sf2) 파일 경로 (UTF-8, null 종료 C 문자열)
/// sample_rate: 게임 엔진에서 사용하는 오디오 샘플레이트 (ex: 44100.0, 48000.0)
/// 반환값: 성공 시 핸들 포인터, 실패 시 null
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_create(
    sf_path: *const c_char,
    sample_rate: c_float,
) -> *mut MimiFfiHandle {
    if sf_path.is_null() {
        return std::ptr::null_mut();
    }

    let path = unsafe {
        match CStr::from_ptr(sf_path).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        }
    };

    match create_mimi_engine(path, sample_rate as f64, |_, _| {}) {
        Ok((handle, context)) => {
            let inner = MimiFfiInner {
                init_state: MimiInitState::Ready { handle, context },
                last_progress: 1.0,
                last_progress_msg: CString::new("Ready").unwrap(),
            };
            Box::into_raw(Box::new(MimiFfiHandle {
                inner: Arc::new(Mutex::new(inner)),
            }))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// 엔진 핸들 생성 (비동기, 논블로킹)
/// 백그라운드 스레드에서 초기화를 시작하고 즉시 핸들을 반환함
/// mimi_ffi_get_init_progress()로 진행 상태를 폴링하고, 완료 후 사용 가능
/// 반환값: 핸들 포인터 (초기화 실패 시에도 반환됨, 프로그레스 함수로 확인 필요)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_create_async(
    sf_path: *const c_char,
    sample_rate: c_float,
) -> *mut MimiFfiHandle {
    if sf_path.is_null() {
        return std::ptr::null_mut();
    }

    let path = match unsafe { CStr::from_ptr(sf_path) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => return std::ptr::null_mut(),
    };

    let inner = Arc::new(Mutex::new(MimiFfiInner {
        init_state: MimiInitState::Initializing,
        last_progress: 0.0,
        last_progress_msg: CString::new("").unwrap(),
    }));

    let inner_clone = Arc::clone(&inner);
    thread::spawn(move || {
        let result = create_mimi_engine(&path, sample_rate as f64, |p, msg| {
            if let Ok(mut guard) = inner_clone.lock() {
                guard.last_progress = p;
                guard.last_progress_msg = CString::new(msg).unwrap_or_default();
                guard.init_state = MimiInitState::Initializing;
            }
        });
        if let Ok(mut guard) = inner_clone.lock() {
            match result {
                Ok((handle, context)) => {
                    guard.last_progress = 1.0;
                    guard.last_progress_msg = CString::new("Ready").unwrap();
                    guard.init_state = MimiInitState::Ready { handle, context };
                }
                Err(_) => {
                    guard.last_progress = 0.0;
                    guard.last_progress_msg = CString::new("Failed").unwrap();
                    guard.init_state = MimiInitState::Failed;
                }
            }
        }
    });

    Box::into_raw(Box::new(MimiFfiHandle { inner }))
}

/// 게임 엔진의 오디오 콜백에서 호출 - 스테레오 인터리브드 f32 버퍼를 채움
/// buffer: L/R 인터리브드 f32 배열 포인터 (buffer[0]=L, buffer[1]=R, ...)
/// frame_count: 스테레오 프레임 수 (buffer 길이 = frame_count * 2)
/// 초기화 미완료 시 무음(silence)을 채움
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_fill_buffer(
    handle: *mut MimiFfiHandle,
    buffer: *mut c_float,
    frame_count: c_uint,
) {
    if handle.is_null() || buffer.is_null() || frame_count == 0 {
        return;
    }

    let h = unsafe { &*handle };
    let sample_count = frame_count as usize * 2;
    let slice = unsafe { std::slice::from_raw_parts_mut(buffer, sample_count) };

    if let Ok(mut guard) = h.inner.lock() {
        if let MimiInitState::Ready { context, .. } = &mut guard.init_state {
            context.fill_buffer(slice);
            return;
        }
    }
    // 초기화 미완료 시 무음
    for s in slice.iter_mut() {
        *s = 0.0;
    }
}

// 초기화 완료 상태에서 핸들 참조 가져오기 (내부 헬퍼)
// None 반환 시 초기화 미완료 또는 잠금 실패
macro_rules! with_ready_handle {
    ($handle:expr, $name:ident => $body:expr) => {
        if $handle.is_null() {
            return Default::default();
        }
        let h = unsafe { &*$handle };
        let guard = match h.inner.lock() {
            Ok(g) => g,
            Err(_) => return Default::default(),
        };
        if let MimiInitState::Ready { handle: $name, .. } = &guard.init_state {
            $body
        } else {
            return Default::default();
        }
    };
}

/// 재생 시작
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_play(handle: *mut MimiFfiHandle) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::Play);
    });
}

/// 일시정지
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_pause(handle: *mut MimiFfiHandle) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::Pause);
    });
}

/// 정지 및 초기화
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_stop(handle: *mut MimiFfiHandle) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::Stop);
    });
}

/// MIDI 파일 로드
/// midi_data: MIDI 바이너리 데이터 포인터
/// data_len: 데이터 길이 (바이트)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_load_song(
    handle: *mut MimiFfiHandle,
    midi_data: *const c_uchar,
    data_len: c_uint,
) {
    if handle.is_null() || midi_data.is_null() || data_len == 0 {
        return;
    }
    let h = unsafe { &*handle };
    if let Ok(guard) = h.inner.lock() {
        if let MimiInitState::Ready { handle, .. } = &guard.init_state {
            let bytes = unsafe {
                std::slice::from_raw_parts(midi_data, data_len as usize).to_vec()
            };
            let _ = handle.send_command(MimiCommand::LoadSong(bytes));
        }
    }
}

/// 조옮김 설정 (-15 ~ +15 반음)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_key(handle: *mut MimiFfiHandle, key: c_int) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::SetKey(key.clamp(-15, 15) as i8));
    });
}

/// 템포 배율 설정 (0.2 ~ 5.0)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_tempo(handle: *mut MimiFfiHandle, tempo: c_float) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::SetTempo(tempo));
    });
}

/// 마스터 볼륨 설정 (0 ~ 100)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_volume(handle: *mut MimiFfiHandle, volume: c_uchar) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::SetVolume(volume));
    });
}

/// 특정 틱 위치로 이동 (Seek)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_seek(handle: *mut MimiFfiHandle, tick: c_uint) {
    with_ready_handle!(handle, h => {
        let _ = h.send_command(MimiCommand::Seek(tick));
    });
}

/// 리듬 모드 설정
/// rhythm: 0=Original, 1=Disco, 2=GoGo, 3=Dance, 4=Techno, 5=Hiphop, 6=Jitterbug, 7=Edm
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_rhythm(handle: *mut MimiFfiHandle, rhythm: c_int) {
    with_ready_handle!(handle, h => {
        let r = match rhythm {
            1 => Rhythm::Disco,
            2 => Rhythm::GoGo,
            3 => Rhythm::Dance,
            4 => Rhythm::Techno,
            5 => Rhythm::Hiphop,
            6 => Rhythm::Jitterbug,
            7 => Rhythm::Edm,
            _ => Rhythm::Original,
        };
        let _ = h.send_command(MimiCommand::SetRhythm(r));
    });
}

/// 비동기 초기화 진행 상태 조회 (Update() 루프에서 매 프레임 폴링)
/// out_progress: 진행률 (0.0 ~ 1.0)을 저장할 포인터
/// out_message: 진행 메시지를 저장할 포인터 (null 종료 C 문자열, 엔진 내부 버퍼 참조)
/// 반환값: 0=초기화 중, 1=완료, -1=실패
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_get_init_progress(
    handle: *const MimiFfiHandle,
    out_progress: *mut c_float,
    out_message: *mut *const c_char,
) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let h = unsafe { &*handle };
    match h.inner.lock() {
        Ok(guard) => {
            let ret = match &guard.init_state {
                MimiInitState::Initializing => 0,
                MimiInitState::Ready { .. } => 1,
                MimiInitState::Failed => -1,
            };
            if !out_progress.is_null() {
                unsafe { *out_progress = guard.last_progress; }
            }
            if !out_message.is_null() {
                unsafe { *out_message = guard.last_progress_msg.as_ptr(); }
            }
            ret
        }
        Err(_) => -1,
    }
}

/// 현재 엔진 상태 조회
/// out: 결과를 저장할 MimiFfiStatus 포인터 (호출자가 메모리 소유)
/// 반환값: 성공 시 1, 실패 시 0
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_get_status(
    handle: *const MimiFfiHandle,
    out: *mut MimiFfiStatus,
) -> c_int {
    if handle.is_null() || out.is_null() {
        return 0;
    }
    let h = unsafe { &*handle };
    let guard = match h.inner.lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };
    if let MimiInitState::Ready { handle, .. } = &guard.init_state {
        match handle.get_status() {
            Ok(status) => {
                let state_int = match status.state {
                    PlayerState::Stopped => 0,
                    PlayerState::Playing => 1,
                    PlayerState::Paused => 2,
                };
                let rhythm_int = match status.current_rhythm {
                    Rhythm::Original  => 0,
                    Rhythm::Disco     => 1,
                    Rhythm::GoGo      => 2,
                    Rhythm::Dance     => 3,
                    Rhythm::Techno    => 4,
                    Rhythm::Hiphop    => 5,
                    Rhythm::Jitterbug => 6,
                    Rhythm::Edm       => 7,
                    Rhythm::Edm2      => 8,
                };
                unsafe {
                    (*out).state           = state_int;
                    (*out).current_tick    = status.current_tick as c_uint;
                    (*out).total_tick      = status.total_tick as c_uint;
                    (*out).current_time_sec = status.current_time.as_secs_f32();
                    (*out).tempo           = status.tempo;
                    (*out).key             = status.key as c_int;
                    (*out).volume          = status.volume;
                    (*out).current_tempo   = status.current_tempo;
                    (*out).is_bs_detected  = if status.is_bs_detected { 1 } else { 0 };
                    (*out).current_rhythm  = rhythm_int;
                }
                1
            }
            Err(_) => 0,
        }
    } else {
        0
    }
}

// 엔진정보 조회
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_get_engine_info(
    handle: *const MimiFfiHandle,
    out: *mut MimiFfiEngineInfo,
) -> c_int {
    if handle.is_null() || out.is_null() {
        return 0;
    }
    let h = unsafe { &*handle };
    let guard = match h.inner.lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };
    if matches!(guard.init_state, MimiInitState::Ready { .. }) {
        let info = mimi_core::get_engine_info();
        unsafe {
            (*out).name = info.name.as_ptr().cast::<c_char>();
            (*out).version = info.version.as_ptr().cast::<c_char>();
            (*out).author = info.author.as_ptr().cast::<c_char>();
            (*out).license = info.license.as_ptr().cast::<c_char>();
        }
        1
    } else {
        0
    }
}

/// 핸들 해제 (반드시 호출해야 메모리 누수 없음)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_destroy(handle: *mut MimiFfiHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)) };
    }
}
