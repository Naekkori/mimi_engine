# MIMI (MIDI Engine for Interactive Music & Instrumentation)

**MIMI**는 저지연(Low Latency) 오디오 합성, 정밀한 실시간 타이밍 제어, 그리고 동적 MIDI 이벤트 조작을 목표로 하는 고성능 미디 엔진 라이브러리입니다. `mimi_core`는 특히 노래방 애플리케이션과 같은 실시간 인터랙티브 음악 시스템에 최적화되어 있습니다.

## 주요 특징 (Key Features)

- **저지연 오디오 합성**: `cpal`을 통한 크로스 플랫폼 오디오 스트리밍과 `oxisynth` 기반의 고성능 사운드폰트 신디사이저를 활용하여 실시간 인터랙션에 최적화된 빠른 응답 속도를 제공합니다.
- **정밀한 실시간 MIDI 시퀀싱**: 샘플 단위의 정확한 MIDI 이벤트 처리 및 재생을 통해 미디 파일의 모든 뉘앙스를 재현합니다.
- **동적 MIDI 이벤트 조작**: 재생 중 마스터 키(Key) 조옮김, 템포(Tempo) 비율 조정, 특정 틱(Tick) 위치로의 점프(Seek) 등 실시간으로 미디 이벤트를 조작할 수 있습니다.
- **노래방 최적화 기능**: 가사 동기화 및 보컬 가이드 트랙 제어와 같은 노래방 애플리케이션에 특화된 이벤트를 UI로 전달하는 기능을 포함합니다.
- **견고한 재생 상태 관리**: 재생(Play), 일시정지(Pause), 정지(Stop) 상태를 명확하게 관리하며, 음 걸림(Note Stuck) 방지 로직을 내장하고 있습니다.
- **사운드폰트 지원**: 표준 SoundFont2(.sf2) 파일을 로드하여 다양한 악기 음색을 사용할 수 있습니다.

## 주요 구성 요소

- `MimiSequencer`: MIDI 파일 파싱 및 이벤트 시퀀싱을 담당합니다.
- `Synth` (oxisynth): 로드된 사운드폰트를 기반으로 오디오를 합성합니다.
- `MimiCommand`: 외부(예: UI)에서 엔진으로 재생 제어 명령(Play, Pause, Stop, SetKey, SetTempo, Seek)을 전달하는 데 사용됩니다.
- `MimiEngineHandle`: MIMI 엔진을 제어하고 현재 상태를 조회하기 위한 인터페이스를 제공합니다.
- `MidiEngineEvent`: 엔진 내부에서 발생하여 UI로 전달될 수 있는 이벤트(예: 가사, 리듬 변경 플래그)를 정의합니다.

## 시작하기 (Getting Started)

### 빌드 방법
```bash
cargo build --release
```

## 📄 라이선스 (License)
Copyright © 2024 MIMI Project. All rights reserved.