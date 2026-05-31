// mimi_ffi/src/lib.rs
// Unreal Engine / Unity / Godot 등 외부 게임 엔진을 위한 C ABI 바인딩
//
// [사용 방식]
// 1. mimi_ffi_create()로 핸들 생성 (samplerate는 게임 엔진에서 알려준 값 사용)
// 2. 게임 엔진의 오디오 콜백 안에서 mimi_ffi_fill_buffer() 호출
// 3. mimi_ffi_send_command_*() 함수로 재생 제어
// 4. mimi_ffi_destroy()로 핸들 해제

use std::ffi::CStr;
use std::os::raw::{c_char, c_float, c_int, c_uchar, c_uint};
use mimi_core::{
    AudioPlaybackContext, MimiCommand, MimiEngineHandle,
    PlayerState, Rhythm, create_mimi_engine,
};

// 외부에서 불투명 포인터로 다루는 핸들 구조체
pub struct MimiFfiHandle {
    handle: MimiEngineHandle,
    context: AudioPlaybackContext,
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

/// 엔진 핸들 생성
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
            Box::into_raw(Box::new(MimiFfiHandle { handle, context }))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// 게임 엔진의 오디오 콜백에서 호출 - 스테레오 인터리브드 f32 버퍼를 채움
/// buffer: L/R 인터리브드 f32 배열 포인터 (buffer[0]=L, buffer[1]=R, ...)
/// frame_count: 스테레오 프레임 수 (buffer 길이 = frame_count * 2)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_fill_buffer(
    handle: *mut MimiFfiHandle,
    buffer: *mut c_float,
    frame_count: c_uint,
) {
    if handle.is_null() || buffer.is_null() || frame_count == 0 {
        return;
    }

    let h = unsafe { &mut *handle };
    let sample_count = frame_count as usize * 2;
    let slice = unsafe { std::slice::from_raw_parts_mut(buffer, sample_count) };
    h.context.fill_buffer(slice);
}

/// 재생 시작
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_play(handle: *mut MimiFfiHandle) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::Play);
    }
}

/// 일시정지
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_pause(handle: *mut MimiFfiHandle) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::Pause);
    }
}

/// 정지 및 초기화
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_stop(handle: *mut MimiFfiHandle) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::Stop);
    }
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

    let h = unsafe { &mut *handle };
    let bytes = unsafe {
        std::slice::from_raw_parts(midi_data, data_len as usize).to_vec()
    };
    let _ = h.handle.send_command(MimiCommand::LoadSong(bytes));
}

/// 조옮김 설정 (-15 ~ +15 반음)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_key(handle: *mut MimiFfiHandle, key: c_int) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::SetKey(key.clamp(-15, 15) as i8));
    }
}

/// 템포 배율 설정 (0.2 ~ 5.0)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_tempo(handle: *mut MimiFfiHandle, tempo: c_float) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::SetTempo(tempo));
    }
}

/// 마스터 볼륨 설정 (0 ~ 100)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_volume(handle: *mut MimiFfiHandle, volume: c_uchar) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::SetVolume(volume));
    }
}

/// 특정 틱 위치로 이동 (Seek)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_seek(handle: *mut MimiFfiHandle, tick: c_uint) {
    if let Some(h) = unsafe { handle.as_mut() } {
        let _ = h.handle.send_command(MimiCommand::Seek(tick));
    }
}

/// 리듬 모드 설정
/// rhythm: 0=Original, 1=Disco, 2=GoGo, 3=Dance, 4=Techno, 5=Hiphop, 6=Jitterbug, 7=Edm
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_set_rhythm(handle: *mut MimiFfiHandle, rhythm: c_int) {
    if let Some(h) = unsafe { handle.as_mut() } {
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
        let _ = h.handle.send_command(MimiCommand::SetRhythm(r));
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
    match h.handle.get_status() {
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
}

/// 핸들 해제 (반드시 호출해야 메모리 누수 없음)
#[unsafe(no_mangle)]
pub extern "C" fn mimi_ffi_destroy(handle: *mut MimiFfiHandle) {
    if !handle.is_null() {
        // Box로 재소유하여 Drop 트리거
        unsafe { drop(Box::from_raw(handle)) };
    }
}
