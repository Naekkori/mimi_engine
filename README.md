# MIMI Engine

**MIMI** (MIDI Engine for Interactive Music & Instrumentation) - Real-time MIDI Sequencing & Audio Synthesis Engine

[English](#english) | [한국어](#한국어)

---

## English

### Overview

MIMI is a high-performance MIDI engine designed for low-latency audio synthesis and precise real-time timing control. It is optimized for real-time interactive music systems such as karaoke applications and supports sample-accurate MIDI event processing.

The audio backend (cpal) is completely decoupled from the core module, allowing integration into game engines with their own audio systems, such as Unreal Engine, Unity, and Godot.

### Key Features

- **Low-Latency Audio Synthesis** - SoundFont software synthesizer based on `fluidlite` with dual ports (Synth A/B) simultaneous synthesis.
- **Sample-Accurate Sequencing** - Precise MIDI event reproduction with frame-by-frame tick progression.
- **Real-Time Control** - Interactive control of key transpose, tempo ratio adjustment, and tick-based seeking during playback.
- **Automated MIDI Standards Detection** - Auto GM / GS / XG format recognition and bank mapping via SysEx analysis.
- **Note Stuck Prevention** - Automatic release of active notes on key change, pause, and stop commands.
- **Real-Time Rhythm Transformation** - Real-time generation of Disco, GoGo, Dance, Techno, Hiphop, Jitterbug, and Edm rhythm patterns based on chord analysis of the $BS (Bass) track.
- **Game Engine Integration** - Simple integration via C ABI FFI as Unreal / Unity / Godot plugins.

### Project Structure

```
mimi_engine/
├── mimi_core/          # Engine Core Library (Audio backend independent)
│   └── src/
│       ├── lib.rs          # Main engine (Synthesis, command processing, fill_buffer API)
│       ├── sequencer.rs    # MIDI parser and sequencer
│       └── rhythm_engine.rs # Real-time rhythm converter (Chord analysis and pattern generation)
├── mimi_cpal/          # cpal audio backend (Standalone player runner)
│   └── src/
│       └── lib.rs          # Stream management and callback bindings
├── mimi_ffi/           # C ABI FFI bindings (For game engine plugins)
│   └── src/
│       └── lib.rs          # Exported extern "C" functions (Builds .dll / .so / .dylib)
├── mimi_player/        # TUI-based reference player
│   └── src/
│       └── main.rs         # Terminal UI powered by ratatui
├── assets/             # MIDI files and Soundfonts (Not included)
│   └── soundfont.sf2
└── Cargo.toml          # Workspace root Cargo.toml
```

#### Dependency Flow

```
mimi_core   ←── mimi_cpal   ←── mimi_player
     ↑
mimi_ffi
```

`mimi_core` does not depend on `cpal`. The audio output mechanism is decided by the upper-level caller.

### Dependencies

#### mimi_core

| Crate | Purpose |
|---|---|
| `fluidlite` | SoundFont2-based software synthesizer |
| `midly` | MIDI (SMF) file parser |
| `crossbeam-channel` | Lock-free command and event queues |
| `anyhow` | Error handling |

#### mimi_cpal

| Crate | Purpose |
|---|---|
| `cpal` | Cross-platform audio I/O library |

#### mimi_player

| Crate | Purpose |
|---|---|
| `ratatui` | Terminal user interface framework |
| `crossterm` | Terminal raw mode and input handling |
| `color-eyre` | Rich panic and error reporting |

### Core APIs

#### mimi_core

##### `create_mimi_engine(sf_path, sample_rate, on_progress) -> (MimiEngineHandle, AudioPlaybackContext)`

Initializes the engine handle and audio context using a SoundFont path and sample rate. Audio output is manually written by binding the returned `AudioPlaybackContext::fill_buffer` to a callback.

##### `AudioPlaybackContext::fill_buffer(&mut self, output: &mut [f32])`

Invoked inside a game engine or audio backend callback. Populates a stereo interleaved (L/R/L/R...) f32 buffer.

##### `MimiEngineHandle`

| Method / Field | Description |
|---|---|
| `send_command(MimiCommand)` | Sends Play, Pause, Stop, SetKey, SetTempo, SetVolume, Seek, LoadSong commands. |
| `get_status()` | Retrieves status, current tick, elapsed time, tempo scale, transpose offset, and master volume. |
| `get_state()` | Returns the current PlayerState. |
| `ui_rx` | Channel receiver for UI events (Lyrics, channel volume levels, tick ticks). |

#### mimi_cpal

##### `spawn_mimi_engine(sf_path, on_progress) -> (MimiEngineHandle, cpal::Stream)`

Used for PC standalone applications. Spawns standard OS output device stream and automatically binds the `fill_buffer` pipeline. The returned `cpal::Stream` must be kept in scope to keep audio running.

### mimi_ffi (Game Engine Integration)

`mimi_ffi` is built as `cdylib` / `staticlib` to generate C-compatible libraries (`.dll` / `.so` / `.dylib`).

#### Basic Flow Example

```c
// 1. Initialize handle with system sample rate
MimiFfiHandle* handle = mimi_ffi_create("assets/soundfont.sf2", 48000.0f);

// 2. Load MIDI file data
mimi_ffi_load_song(handle, midi_bytes, midi_len);

// 3. Start playback
mimi_ffi_play(handle);

// 4. Fill buffer inside engine audio callback
mimi_ffi_fill_buffer(handle, audio_buffer, frame_count);

// 5. Cleanup when finished
mimi_ffi_destroy(handle);
```

#### FFI Function Reference

| Function | Description |
|---|---|
| `mimi_ffi_create(sf_path, sample_rate)` | Spawns a handle. Returns null on failure. |
| `mimi_ffi_destroy(handle)` | Deallocates engine objects (Must be called to prevent memory leaks). |
| `mimi_ffi_fill_buffer(handle, buffer, frame_count)` | Populates output audio buffer. |
| `mimi_ffi_load_song(handle, midi_data, data_len)` | Loads MIDI binary into the sequencer. |
| `mimi_ffi_play(handle)` | Resumes playback. |
| `mimi_ffi_pause(handle)` | Pauses playback. |
| `mimi_ffi_stop(handle)` | Stops playback and resets state. |
| `mimi_ffi_set_key(handle, key)` | Shifts pitch (-15 to +15). |
| `mimi_ffi_set_tempo(handle, tempo)` | Multiplies tempo speed (0.2 to 5.0). |
| `mimi_ffi_set_volume(handle, volume)` | Adjusts master volume level (0 to 100). |
| `mimi_ffi_seek(handle, tick)` | Seeks to a specific tick position. |
| `mimi_ffi_set_rhythm(handle, rhythm)` | Switches rhythm styles (See table below). |
| `mimi_ffi_get_status(handle, out)` | Queries engine status metrics. 1 on success, 0 on failure. |

#### Rhythm Codes

| Code | Style |
|---|---|
| 0 | Original (No transformation) |
| 1 | Disco |
| 2 | GoGo |
| 3 | Dance |
| 4 | Techno |
| 5 | Hiphop |
| 6 | Jitterbug |
| 7 | Edm |

#### `MimiFfiStatus` Structure

```c
typedef struct {
    int   state;            // 0=Stopped, 1=Playing, 2=Paused
    unsigned int current_tick;
    unsigned int total_tick;
    float current_time_sec;
    float tempo;
    int   key;
    unsigned char volume;
    int   current_tempo;    // µs/beat
    int   is_bs_detected;   // 0 or 1
    int   current_rhythm;   // Matching rhythm codes
} MimiFfiStatus;
```

#### Build Instructions

```bash
# General Workspace Build
cargo build --release

# Run TUI Player
cargo run -p mimi_player --release

# Build C FFI Library
cargo build --release -p mimi_ffi
```

---

## 한국어

### 개요

MIMI는 저지연 오디오 합성과 정밀한 실시간 타이밍 제어를 목표로 하는 고성능 미디 엔진이다. 노래방 애플리케이션과 같은 실시간 인터랙티브 음악 시스템에 최적화되어 있으며, 샘플 단위의 정확한 MIDI 이벤트 처리를 지원한다.

오디오 백엔드(cpal)가 코어와 완전히 분리되어 있어 Unreal Engine, Unity, Godot 등 자체 오디오 시스템을 가진 게임 엔진에서도 사용할 수 있다.

### 주요 기능

- **저지연 오디오 합성** - `fluidlite` 기반 사운드폰트 신디사이저, 듀얼 포트(Synth A/B) 동시 합성
- **샘플 단위 시퀀싱** - 프레임별 틱 전진으로 MIDI 이벤트를 정밀하게 재현
- **실시간 제어** - 재생 중 키 조옮김(Transpose), 템포 비율 조정, 틱 위치 점프(Seek)
- **MIDI 규격 자동 감지** - SysEx 분석을 통한 GM / GS / XG 포맷 자동 판별 및 뱅크 매핑
- **음 걸림 방지** - 키 변경, 일시정지, 정지 시 활성 노트 자동 해제
- **실시간 리듬 변환** - $BS(베이스) 트랙 코드 분석 기반으로 Disco, GoGo, Dance, Techno, Hiphop, Jitterbug, Edm 리듬 패턴 실시간 생성
- **게임 엔진 연동** - C ABI FFI를 통해 Unreal / Unity / Godot 플러그인으로 통합 가능

### 프로젝트 구조

```
mimi_engine/
├── mimi_core/          # 엔진 코어 라이브러리 (오디오 백엔드 독립적)
│   └── src/
│       ├── lib.rs          # 엔진 메인 (합성, 명령 처리, fill_buffer API)
│       ├── sequencer.rs    # MIDI 파서 및 시퀀서
│       └── rhythm_engine.rs # 실시간 리듬 변환 엔진 (코드 분석 및 패턴 생성)
├── mimi_cpal/          # cpal 오디오 백엔드 (PC 독립 실행용)
│   └── src/
│       └── lib.rs          # cpal 장치 열기 및 fill_buffer 콜백 연결
├── mimi_ffi/           # C ABI FFI 바인딩 (게임 엔진 플러그인용)
│   └── src/
│       └── lib.rs          # extern "C" 함수 노출 (.dll / .so / .dylib 빌드)
├── mimi_player/        # TUI 기반 레퍼런스 플레이어
│   └── src/
│       └── main.rs         # ratatui 기반 터미널 UI
├── assets/             # MIDI 파일 및 사운드폰트 (미포함)
│   └── soundfont.sf2
└── Cargo.toml          # 워크스페이스 루트
```

#### 크레이트 의존 관계

```
mimi_core   ←── mimi_cpal   ←── mimi_player
     ↑
mimi_ffi
```

`mimi_core`는 cpal에 의존하지 않으며, 오디오 출력 방식은 상위 레이어가 결정한다.

### 의존성

#### mimi_core

| 크레이트 | 용도 |
|---------|------|
| `fluidlite` | SoundFont2 기반 소프트웨어 신디사이저 |
| `midly` | MIDI(SMF) 파일 파싱 |
| `crossbeam-channel` | 스레드 간 lock-free 명령/이벤트 채널 |
| `anyhow` | 오류 처리 |

#### mimi_cpal

| 크레이트 | 용도 |
|---------|------|
| `cpal` | 크로스 플랫폼 오디오 출력 |

#### mimi_player

| 크레이트 | 용도 |
|---------|------|
| `ratatui` | 터미널 UI 프레임워크 |
| `crossterm` | 터미널 입력 처리 |
| `color-eyre` | 오류 리포팅 |

### 핵심 API

#### mimi_core

##### `create_mimi_engine(sf_path, sample_rate, on_progress) -> (MimiEngineHandle, AudioPlaybackContext)`

사운드폰트 경로와 샘플레이트를 받아 엔진 핸들과 오디오 컨텍스트를 생성한다. 오디오 출력은 반환된 `AudioPlaybackContext::fill_buffer`를 통해 상위 레이어가 직접 연결한다.

##### `AudioPlaybackContext::fill_buffer(&mut self, output: &mut [f32])`

게임 엔진 또는 오디오 백엔드의 콜백에서 호출한다. 스테레오 인터리브드(L/R/L/R...) f32 버퍼를 채운다.

##### `MimiEngineHandle`

| 메서드 / 필드 | 설명 |
|-------|------|
| `send_command(MimiCommand)` | Play, Pause, Stop, SetKey, SetTempo, SetVolume, Seek, LoadSong 명령 전송 |
| `get_status()` | 현재 상태, 틱 위치, 경과 시간, 템포 배율, 키 오프셋, 볼륨 상태 조회 |
| `get_state()` | 현재 PlayerState 반환 |
| `ui_rx` | 가사, 채널 레벨, 틱 업데이트 등 UI 이벤트 수신 채널 |

#### mimi_cpal

##### `spawn_mimi_engine(sf_path, on_progress) -> (MimiEngineHandle, cpal::Stream)`

PC 독립 실행 환경에서 사용한다. cpal 기본 출력 장치를 열고 `fill_buffer`를 자동으로 연결한다. 반환된 `cpal::Stream`은 드롭되면 오디오가 끊기므로 상위 스코프에서 보관해야 한다.

### mimi_ffi (게임 엔진 연동)

`mimi_ffi`는 `cdylib` / `staticlib`으로 빌드되어 C 호환 `.dll` / `.so` / `.dylib`를 생성한다.

#### 기본 흐름

```c
// 1. 게임 엔진 오디오 시스템의 샘플레이트를 넘겨서 핸들 생성
MimiFfiHandle* handle = mimi_ffi_create("assets/soundfont.sf2", 48000.0f);

// 2. MIDI 파일 로드
mimi_ffi_load_song(handle, midi_bytes, midi_len);

// 3. 재생 시작
mimi_ffi_play(handle);

// 4. 게임 엔진 오디오 콜백 안에서 버퍼 채우기 (스테레오 인터리브드 f32)
mimi_ffi_fill_buffer(handle, audio_buffer, frame_count);

// 5. 사용 종료 시 반드시 해제
mimi_ffi_destroy(handle);
```

#### C ABI 함수 목록

| 함수 | 설명 |
|------|------|
| `mimi_ffi_create(sf_path, sample_rate)` | 핸들 생성. 실패 시 null 반환 |
| `mimi_ffi_destroy(handle)` | 핸들 해제 (반드시 호출) |
| `mimi_ffi_fill_buffer(handle, buffer, frame_count)` | 오디오 콜백에서 버퍼 채우기 |
| `mimi_ffi_load_song(handle, midi_data, data_len)` | MIDI 바이너리 로드 |
| `mimi_ffi_play(handle)` | 재생 시작 |
| `mimi_ffi_pause(handle)` | 일시정지 |
| `mimi_ffi_stop(handle)` | 정지 및 초기화 |
| `mimi_ffi_set_key(handle, key)` | 조옮김 설정 (-15 ~ +15) |
| `mimi_ffi_set_tempo(handle, tempo)` | 템포 배율 설정 (0.2 ~ 5.0) |
| `mimi_ffi_set_volume(handle, volume)` | 마스터 볼륨 설정 (0 ~ 100) |
| `mimi_ffi_seek(handle, tick)` | 틱 위치로 이동 |
| `mimi_ffi_set_rhythm(handle, rhythm)` | 리듬 모드 설정 (아래 표 참조) |
| `mimi_ffi_get_status(handle, out)` | 상태 조회. 성공 시 1, 실패 시 0 반환 |

#### 리듬 모드 상수

| 값 | 리듬 |
|----|------|
| 0 | Original (원곡) |
| 1 | Disco |
| 2 | GoGo |
| 3 | Dance |
| 4 | Techno |
| 5 | Hiphop |
| 6 | Jitterbug |
| 7 | Edm (EDM) |

#### `MimiFfiStatus` 구조체

```c
typedef struct {
    int   state;            // 0=Stopped, 1=Playing, 2=Paused
    unsigned int current_tick;
    unsigned int total_tick;
    float current_time_sec;
    float tempo;
    int   key;
    unsigned char volume;
    int   current_tempo;    // µs/beat
    int   is_bs_detected;   // 0 또는 1
    int   current_rhythm;   // 위 리듬 모드 상수와 동일
} MimiFfiStatus;
```

#### mimi_ffi 빌드

```bash
# .dll / .so / .dylib 생성
cargo build --release -p mimi_ffi
```

빌드 결과물은 `target/release/` 아래 플랫폼별 확장자로 생성된다.

---

### `MimiCommand`

```rust
Play                    // 재생 시작
Pause                   // 일시정지 (음 걸림 방지 처리 포함)
Stop                    // 정지 및 초기화
SetKey(i8)              // 조옮김 오프셋 설정 (-15 ~ +15)
SetTempo(f32)           // 템포 배율 설정 (0.2 ~ 5.0)
SetVolume(u8)           // 마스터 볼륨 설정 (0 ~ 100)
Seek(u32)               // 특정 틱 위치로 점프
LoadSong(Vec<u8>)       // 새로운 MIDI 바이너리 로드 및 대기
SetRhythm(Rhythm)       // 실시간 리듬 모드 변경
```

### `Rhythm`

```rust
Original    // 원곡 그대로 (리듬 변환 꺼짐)
Disco       // 디스코
GoGo        // 고고
Dance       // 댄스
Techno      // 테크노
Hiphop      // 힙합
Jitterbug   // 지르박
Edm         // EDM
```

### `MidiEngineEvent`

엔진이 UI 쪽(`ui_rx`)으로 송출하는 이벤트 타입이다.

```rust
MidiPlay { port, channel, is_drum_channel, kind }  // MIDI 이벤트 원본
TempoChange { tempo }                              // 템포 변경
SmfKaraokeText { text }                           // 노래방 가사
MidiReset                                          // 시스템 리셋
TickUpdate { current_tick, total_tick }            // 재생 진행 위치
ChannelLevel { port, levels }                      // 채널 레벨미터
SetDrumChannel { port, channel, is_drum }          // 드럼 채널 설정 변경
ChordUpdate { root_pitch, is_minor }               // 실시간 코드 상태 (디버그)
FluidsynthWarning { message }                      // Fluidsynth/Fluidlite 경고 및 에러 메시지
```

### `MimiEngineStatus`

`get_status()`로 조회하는 통합 상태 구조체이다.

| 필드 | 타입 | 설명 |
|------|------|------|
| `state` | `PlayerState` | 현재 재생 상태 (Stopped / Playing / Paused) |
| `current_tick` | `u64` | 현재 틱 위치 |
| `total_tick` | `u64` | 전체 틱 수 |
| `current_time` | `Duration` | 경과 시간 |
| `tempo` | `f32` | 현재 템포 배율 |
| `key` | `i8` | 현재 조옮김 오프셋 |
| `volume` | `u8` | 현재 마스터 볼륨 |
| `current_rhythm` | `Rhythm` | 현재 리듬 모드 |
| `current_tempo` | `i32` | MIDI 파일 원본 템포 (µs/beat) |
| `is_bs_detected` | `bool` | $BS(베이스) 트랙 검출 여부 |

## 빌드 및 실행

```bash
# 전체 빌드
cargo build --release

# TUI 플레이어 실행
cargo run -p mimi_player --release

# 게임 엔진용 FFI 라이브러리 빌드
cargo build --release -p mimi_ffi
```

`assets/` 디렉토리에 `.mid` 파일과 `soundfont.sf2`를 배치한 후 플레이어를 실행한다.

### 플레이어 조작키

| 키 / 마우스 | 동작 |
|---|------|
| `↑` / `↓` | 파일 선택 |
| `Enter` | 파일 로드 및 재생 |
| `Space` | 재생 / 일시정지 토글 |
| `s` | 정지 |
| `,` / `.` | 키(음정) 내림 / 올림 |
| `[` / `]` | 템포 내림 / 올림 (0.1 단위) |
| `-` / `=` | 볼륨 내림 / 올림 (5 단위) |
| `←` / `→` | 100틱 뒤로 / 앞으로 이동 (`Shift` 조합 시 500틱 단위) |
| 마우스 왼쪽 클릭/드래그 | 재생 상태바에서 해당 위치로 직접 이동 (Seek) |
| `Esc` | 재생 정지 및 파일 리스트 화면으로 복귀 |

## 라이선스

### 오픈소스 라이선스

| 크레이트 | 라이선스 | 라이선스 사본 |
|---------|---------|-------------|
| `fluidlite` | LGPL-2.1 | [LICENSE](https://github.com/katyo/fluidlite/blob/master/LICENSE) |
| `midly` | Unlicense | [LICENSE](https://github.com/kovaxis/midly/blob/master/LICENSE) |
| `cpal` | Apache-2.0 | [LICENSE](https://github.com/RustAudio/cpal/blob/master/LICENSE) |
| `crossbeam-channel` | MIT / Apache-2.0 | [MIT](https://github.com/crossbeam-rs/crossbeam/blob/master/LICENSE-MIT) / [Apache-2.0](https://github.com/crossbeam-rs/crossbeam/blob/master/LICENSE-APACHE) |
| `ratatui` | MIT | [LICENSE](https://github.com/ratatui/ratatui/blob/main/LICENSE) |
| `crossterm` | MIT | [LICENSE](https://github.com/crossterm-rs/crossterm/blob/master/LICENSE) |
| `anyhow` | MIT / Apache-2.0 | [MIT](https://github.com/dtolnay/anyhow/blob/master/LICENSE-MIT) / [Apache-2.0](https://github.com/dtolnay/anyhow/blob/master/LICENSE-APACHE) |
| `color-eyre` | MIT / Apache-2.0 | [MIT](https://github.com/eyre-rs/eyre/blob/master/LICENSE-MIT) / [Apache-2.0](https://github.com/eyre-rs/eyre/blob/master/LICENSE-APACHE) |
