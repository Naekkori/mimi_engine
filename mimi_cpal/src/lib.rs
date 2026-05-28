// mimi_cpal/src/lib.rs
// cpal 오디오 백엔드와 mimi_core를 연결하는 레이어
// 게임 엔진 등에서 mimi_core를 직접 사용할 때는 이 크레이트를 쓰지 않아도 됨

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use mimi_core::{MimiEngineHandle, create_mimi_engine};
use msgbox::create;

/// cpal 기본 출력 장치에 mimi_core 엔진을 연결하고 스트림을 시작함
/// 반환된 cpal::Stream은 호출자가 드롭되지 않도록 직접 관리해야 함
pub fn spawn_mimi_engine(
    sf_path: &str,
    mut on_progress: impl FnMut(f32, &str) + Send + 'static,
) -> Result<(MimiEngineHandle, cpal::Stream), ()> {
    on_progress(0.05, "오디오 출력 장치 및 스트림 초기화 중...");

    // cpal 기본 출력 장치 열기
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let _ = create("오류", "오디오 출력 장치를 찾을 수 없습니다.", msgbox::IconType::Error);
            return Err(());
        }
    };
    let config = device.default_output_config().map_err(|e| {
        let _ = create("오류", &format!("기본 오디오 설정 조회 실패: {}", e), msgbox::IconType::Error);
    })?;
    let sample_rate = config.sample_rate() as f64;
    let channels = config.channels();

    // 스테레오 확인
    if channels != 2u16 {
        let _ = create("오류", "MIMI 엔진은 현재 스테레오(2채널) 출력 장치만 지원합니다.", msgbox::IconType::Error);
    }

    let cpal_config: cpal::StreamConfig = config.into();

    // mimi_core 엔진 생성 (cpal 없이 순수 합성 컨텍스트만)
    let (handle, mut context) = create_mimi_engine(sf_path, sample_rate, |p, msg| {
        on_progress(p * 0.9 + 0.05, msg);
    }).map_err(|_| ())?;

    // cpal 스트림에 fill_buffer 콜백으로 연결
    let error_callback = move |err: cpal::StreamError| {
        let _ = create("오류", &format!("오디오 스트림 에러 발생: {}", err), msgbox::IconType::Error);
    };
    let stream = device.build_output_stream(
        &cpal_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            context.fill_buffer(data);
        },
        error_callback,
        None,
    ).map_err(|e| {
        let _ = create("오류", &format!("오디오 스트림 빌드 실패: {}", e), msgbox::IconType::Error);
    })?;

    // 스트림 즉시 가동
    stream.play().map_err(|e| {
        let _ = create("오류", &format!("오디오 재생 시작 실패: {}", e), msgbox::IconType::Error);
    })?;

    on_progress(1.0, "엔진 초기화 완료!");

    // cpal::Stream의 수명이 다하면 소리가 끊기므로 호출자에서 반드시 보관해야 함
    Ok((handle, stream))
}
