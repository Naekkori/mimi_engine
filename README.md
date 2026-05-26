# MIMI Engine

**MIMI** (MIDI Engine for Interactive Music & Instrumentation) - 실시간 MIDI 시퀀싱 및 오디오 합성 엔진

## 개요

MIMI는 저지연 오디오 합성과 정밀한 실시간 타이밍 제어를 목표로 하는 고성능 미디 엔진이다. 노래방 애플리케이션과 같은 실시간 인터랙티브 음악 시스템에 최적화되어 있으며, 샘플 단위의 정확한 MIDI 이벤트 처리를 지원한다.

## 주요 기능

- **저지연 오디오 합성** - `fluidlite` 기반 사운드폰트 신디사이저, 듀얼 포트(Synth A/B) 동시 합성
- **샘플 단위 시퀀싱** - 프레임별 틱 전진으로 MIDI 이벤트를 정밀하게 재현
- **실시간 제어** - 재생 중 키 조옮김(Transpose), 템포 비율 조정, 틱 위치 점프(Seek)
- **MIDI 규격 자동 감지** - SysEx 분석을 통한 GM / GS / XG 포맷 자동 판별 및 뱅크 매핑
- **음 걸림 방지** - 키 변경, 일시정지, 정지 시 활성 노트 자동 해제
- **실시간 리듬 변환** - $BS(베이스) 트랙 코드 분석 기반으로 Disco, GoGo, Dance, Techno, Hiphop, Jitterbug 리듬 패턴 실시간 생성

## 프로젝트 구조

```
mimi_engine/
├── mimi_core/          # 엔진 코어 라이브러리
│   └── src/
│       ├── lib.rs          # 엔진 메인 (합성, 명령 처리, CPAL 스트림)
│       ├── sequencer.rs    # MIDI 파서 및 시퀀서
│       └── rhythm_engine.rs # 실시간 리듬 변환 엔진 (코드 분석 및 패턴 생성)
├── mimi_player/        # TUI 기반 레퍼런스 플레이어
│   └── src/
│       └── main.rs         # ratatui 기반 터미널 UI
├── assets/             # MIDI 파일 및 사운드폰트 (미포함)
│   └── soundfont.sf2
└── Cargo.toml          # 워크스페이스 루트
```

## 의존성

| 크레이트 | 용도 |
|---------|------|
| `fluidlite` | SoundFont2 기반 소프트웨어 신디사이저 |
| `midly` | MIDI(SMF) 파일 파싱 |
| `cpal` | 크로스 플랫폼 오디오 출력 |
| `crossbeam-channel` | 스레드 간 lock-free 명령/이벤트 채널 |
| `ratatui` | 터미널 UI 프레임워크 |
| `crossterm` | 터미널 입력 처리 (mimi_player) |
| `color-eyre` | 오류 리포팅 (mimi_player) |

## 핵심 API

### `spawn_mimi_engine(sf_path, midi_bytes) -> (MimiEngineHandle, Stream)`

사운드폰트 경로와 MIDI 바이트 데이터를 받아 오디오 엔진을 구동한다. `MimiEngineHandle`과 CPAL `Stream`을 반환하며, Stream의 수명이 다하면 오디오가 중단되므로 상위 스코프에서 관리해야 한다.

### `MimiEngineHandle`

| 메서드 / 필드 | 설명 |
|-------|------|
| `send_command(MimiCommand)` | Play, Pause, Stop, SetKey, SetTempo, SetVolume, Seek, LoadSong 명령 전송 |
| `get_status()` | 현재 상태, 틱 위치, 경과 시간, 템포 배율, 키 오프셋, 볼륨 상태 조회 |
| `get_state()` | 현재 PlayerState 반환 |
| `ui_rx` | 가사, 채널 레벨, 틱 업데이트 등 UI 이벤트 수신 채널 |

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
# 빌드
cargo build --release

# TUI 플레이어 실행
cargo run -p mimi_player --release
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
