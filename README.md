# MIMI Engine

**MIMI** (MIDI Engine for Interactive Music & Instrumentation) - 실시간 MIDI 시퀀싱 및 오디오 합성 엔진

## 개요

MIMI는 저지연 오디오 합성과 정밀한 실시간 타이밍 제어를 목표로 하는 고성능 미디 엔진이다. 노래방 애플리케이션과 같은 실시간 인터랙티브 음악 시스템에 최적화되어 있으며, 샘플 단위의 정확한 MIDI 이벤트 처리를 지원한다.

## 주요 기능

- **저지연 오디오 합성** - `fluidlite` 기반 사운드폰트 신디사이저, 듀얼 포트(Synth A/B) 동시 합성
- **샘플 단위 시퀀싱** - 프레임별 틱 전진으로 MIDI 이벤트를 정밀하게 재현
- **실시간 제어** - 재생 중 키 조옮김(Transpose), 템포 비율 조정, 틱 위치 점프(Seek)
- **MIDI 규격 자동 감지** - SysEx 분석을 통한 GM / GS / XG 포맷 자동 판별 및 뱅크 매핑
- **노래방 지원** - SMF 가사(Lyric/Text) 이벤트 파싱 및 UI 전달
- **음 걸림 방지** - 키 변경, 일시정지, 정지 시 활성 노트 자동 해제
- **채널 레벨 모니터링** - 포트별 16채널 velocity 추적 및 소프트 감쇠

## 프로젝트 구조

```
mimi_engine/
├── mimi_core/          # 엔진 코어 라이브러리
│   └── src/
│       ├── lib.rs          # 엔진 메인 (합성, 명령 처리, CPAL 스트림)
│       └── sequencer.rs    # MIDI 파서 및 시퀀서
├── mimi_player/        # TUI 기반 레퍼런스 플레이어
│   └── src/
│       └── main.rs         # ratatui 기반 터미널 UI
├── assets/             # MIDI 파일 및 사운드폰트
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
| `ratatui` | 터미널 UI 프레임워크 (mimi_player) |
| `crossterm` | 터미널 입력 처리 (mimi_player) |

## 핵심 API

### `spawn_mimi_engine(sf_path, midi_bytes) -> (MimiEngineHandle, Stream)`

사운드폰트 경로와 MIDI 바이트 데이터를 받아 오디오 엔진을 구동한다. `MimiEngineHandle`과 CPAL `Stream`을 반환하며, Stream의 수명이 다하면 오디오가 중단되므로 상위 스코프에서 관리해야 한다.

### `MimiEngineHandle`

| 메서드 | 설명 |
|-------|------|
| `send_command(MimiCommand)` | Play, Pause, Stop, SetKey, SetTempo, Seek 명령 전송 |
| `get_status()` | 현재 상태, 틱 위치, 경과 시간 조회 |
| `get_state()` | 현재 PlayerState 반환 |
| `ui_rx` | 가사, 채널 레벨, 틱 업데이트 등 UI 이벤트 수신 채널 |

### `MimiCommand`

```rust
Play                // 재생 시작
Pause               // 일시정지
Stop                // 정지 및 초기화
SetKey(i8)          // 조옮김 오프셋 설정
SetTempo(f32)       // 템포 배율 (1.0 = 정속)
Seek(u32)           // 특정 틱 위치로 점프
```

## 빌드 및 실행

```bash
# 빌드
cargo build --release

# TUI 플레이어 실행
cargo run -p mimi_player --release
```

`assets/` 디렉토리에 `.mid` 파일과 `soundfont.sf2`를 배치한 후 플레이어를 실행한다.

### 플레이어 조작키

| 키 | 동작 |
|---|------|
| `↑` / `↓` | 파일 선택 |
| `Enter` | 재생 |
| `Space` | 재생 / 일시정지 토글 |
| `s` | 정지 |
| `,` / `.` | 키 내림 / 올림 |
| `Esc` | 리스트로 복귀 |

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
