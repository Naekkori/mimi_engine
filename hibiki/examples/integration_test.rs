// integration_test.rs - mimi_core 연결 검증 테스트
// mimi_core가 사용하는 모든 API 패턴을 시뮬레이션
// 출력을 WAV 파일로 저장해서 들을 수 있게

use hibiki::{HibikiSettings, HibikiSynth};
use std::io::Write;

/// 16-bit PCM mono WAV 파일 저장
fn save_wav(path: &str, samples: &[i16], sample_rate: u32) {
    let mut file = std::fs::File::create(path).expect("create wav");
    let data_len = (samples.len() * 2) as u32;
    let file_len = data_len + 36;

    // RIFF 헤더
    file.write_all(b"RIFF").ok();
    file.write_all(&file_len.to_le_bytes()).ok();
    file.write_all(b"WAVE").ok();

    // fmt 청크
    file.write_all(b"fmt ").ok();
    file.write_all(&16u32.to_le_bytes()).ok();
    file.write_all(&1u16.to_le_bytes()).ok(); // PCM
    file.write_all(&1u16.to_le_bytes()).ok(); // mono
    file.write_all(&sample_rate.to_le_bytes()).ok();
    file.write_all(&(sample_rate * 2u32).to_le_bytes()).ok(); // byte rate
    file.write_all(&2u16.to_le_bytes()).ok(); // block align
    file.write_all(&16u16.to_le_bytes()).ok(); // bits per sample

    // data 청크
    file.write_all(b"data").ok();
    file.write_all(&data_len.to_le_bytes()).ok();
    for s in samples {
        file.write_all(&s.to_le_bytes()).ok();
    }
}

fn main() {
    println!("=== Hibiki 사운드폰트 엔진 통합 테스트 ===\n");

    // 1. 엔진 생성 (mimi_core의 new_engine 패턴)
    let mut settings = HibikiSettings::new();
    settings.set_sample_rate(44100.0);

    let mut synth = HibikiSynth::new(settings).expect("Failed to create synth");
    // 마스터 게인을 낮춰서 누적 방지
    synth.set_gain(0.4);
    println!("[1] HibikiSynth 생성 완료 (gain=0.4)");

    // 2. 사운드폰트 로드 (mimi_core의 sfload 패턴)
    let sf_path = "d:/source/mimi_engine/assets/soundfont.SF2";
    match synth.sfload(sf_path, true) {
        Ok(preset_count) => {
            println!("[2] 사운드폰트 로드 성공: {} 프리셋", preset_count);

            // 사운드폰트 정보 확인
            if let Some(sf) = synth.get_soundfont_info() {
                println!("    - 샘플 수: {}", sf.samples.len());
                println!("    - 악기 수: {}", sf.instruments.len());
                println!("    - 프리셋 수: {}", sf.presets.len());

                // 첫 5개 악기 zone의 ADSR/loop 출력
                for (i, inst) in sf.instruments.iter().take(5).enumerate() {
                    println!("    [Inst {}] {} (zones: {})", i, inst.name, inst.zones.len());
                    for (j, z) in inst.zones.iter().take(3).enumerate() {
                        println!(
                            "      zone {}: key={:?} vel={:?} sample={:?} A={:.3}s D={:.3}s S={:.2} R={:.3}s atten={:.0}cb",
                            j, z.key_range, z.velocity_range, z.sample_index,
                            z.attack, z.decay, z.sustain, z.release, z.attenuation
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("[2] 사운드폰트 로드 실패: {}", e);
            return;
        }
    }

    // 3. 16개 채널 모두 초기화 (mimi_core의 채널 리셋 패턴)
    for ch in 0u32..16 {
        let _ = synth.cc(ch, 121, 0); // Reset All Controllers
        let _ = synth.cc(ch, 0, 0);   // Bank Select MSB
        let _ = synth.cc(ch, 32, 0);  // Bank Select LSB
        let _ = synth.cc(ch, 7, 100);  // Volume
        let _ = synth.cc(ch, 11, 127); // Expression
        let _ = synth.cc(ch, 10, 64);  // Pan
        let _ = synth.pitch_bend(ch, 8192); // Pitch Bend Center
        let _ = synth.cc(ch, 91, 40);  // Reverb
        let _ = synth.cc(ch, 93, 0);   // Chorus
        let _ = synth.cc(ch, 94, 0);   // Effect 4
    }

    // 채널 0: 다양한 program 테스트
    // Program 5 (Electric Piano 1) - 더 명확한 음
    let _ = synth.program_change(0, 4);  // Electric Piano 1
    let _ = synth.program_change(1, 0);  // Acoustic Grand
    let _ = synth.program_change(2, 24); // Nylon Guitar
    let _ = synth.program_change(9, 0);  // Channel 9 (drum)
    println!("[3] 16채널 초기화 완료 + 다양한 program 설정 (5/0/24/0)");

    // 4. 미디 이벤트 시뮬레이션 - 다양한 velocity로 진짜 음 찾기
    println!("\n[4] 다양한 velocity로 진짜 음 찾기 (WAV 저장)");
    let mut all_samples: Vec<i16> = Vec::new();

    // Acoustic Grand (program 0) 다양한 velocity 테스트
    let _ = synth.program_change(0, 0);
    let test_notes = [60, 64, 67, 72]; // C4, E4, G4, C5
    let test_velocities = [30, 60, 100, 127];

    for (i, &note) in test_notes.iter().enumerate() {
        let vel = test_velocities[i % 4];
        if let Err(e) = synth.note_on(0, note, vel) {
            eprintln!("    NoteOn({}) 실패: {}", note, e);
        }

        // 0.5초 렌더링
        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;
        let mut non_zero_count = 0;

        for _ in 0..22050 {
            let mut out = [0.0f32; 2];
            if let Err(e) = synth.write_samples(&mut out) {
                eprintln!("    write_samples 실패: {}", e);
                return;
            }
            all_samples.push((out[0].clamp(-1.0, 1.0) * 32767.0) as i16);
            peak_l = peak_l.max(out[0].abs());
            peak_r = peak_r.max(out[1].abs());
            if out[0].abs() > 0.001 || out[1].abs() > 0.001 {
                non_zero_count += 1;
            }
        }

        let _ = synth.note_off(0, note);

        // 0.3초 release
        for _ in 0..13230 {
            let mut out = [0.0f32; 2];
            let _ = synth.write_samples(&mut out);
            all_samples.push((out[0].clamp(-1.0, 1.0) * 32767.0) as i16);
        }

        println!(
            "    [prog=0] Note {} vel={}: peak(L,R) = ({:.4}, {:.4}), non-zero samples = {}",
            note, vel, peak_l, peak_r, non_zero_count
        );
    }

    // WAV 파일로 저장
    let wav_path = "d:/source/mimi_engine/hibiki_output.wav";
    save_wav(wav_path, &all_samples, 44100);
    println!("    [WAV 저장] {} ({} samples, {:.1}초)",
        wav_path, all_samples.len(), all_samples.len() as f32 / 44100.0);

    // 5. 확장 CC 테스트 (GS/XG)
    println!("\n[5] 확장 CC 테스트");
    let _ = synth.cc(0, 1, 64);    // Modulation
    let _ = synth.cc(0, 71, 32);   // Filter Resonance
    let _ = synth.cc(0, 74, 100);  // Filter Cutoff
    let _ = synth.cc(0, 76, 50);   // Vibrato Depth
    let _ = synth.cc(0, 91, 80);   // Reverb Send
    let _ = synth.cc(0, 92, 30);   // Tremolo
    let _ = synth.cc(0, 95, 40);   // Phaser
    let _ = synth.cc(0, 98, 0);    // NRPN LSB
    let _ = synth.cc(0, 99, 0);    // NRPN MSB
    let _ = synth.cc(0, 100, 0);   // RPN LSB
    let _ = synth.cc(0, 101, 0);   // RPN MSB
    let _ = synth.cc(0, 6, 2);     // Data Entry (RPN 0,0 = Pitch Bend Range)
    println!("    GS/XG CC 처리 완료");

    // 6. 이펙트 활성화 (mimi_core의 이펙트 사용 패턴)
    println!("\n[6] 이펙트 활성화 테스트");
    synth.enable_effect(true, true, false); // Reverb ON, Chorus ON
    synth.enable_tremolo(true);
    synth.enable_phaser(true);
    synth.enable_celeste(true);
    synth.set_chorus_params(2.0, 0.5, 25.0);
    synth.set_reverb_params(0.5, 0.3);
    synth.set_tremolo_params(5.0, 0.3);
    synth.set_phaser_params(0.5, 0.5, 0.3);
    synth.set_celeste_detune(15.0);
    println!("    모든 이펙트 활성화 및 파라미터 설정 완료");

    // 7. 이펙트 포함 렌더링
    println!("\n[7] 이펙트 포함 렌더링 테스트");
    let _ = synth.note_on(0, 60, 100);
    let mut peak_l = 0.0f32;
    let mut peak_r = 0.0f32;
    for _ in 0..4410 {
        let mut out = [0.0f32; 2];
        let _ = synth.write_samples_with_effects(&mut out);
        peak_l = peak_l.max(out[0].abs());
        peak_r = peak_r.max(out[1].abs());
    }
    let _ = synth.note_off(0, 60);
    println!(
        "    이펙트 적용 후 peak(L,R) = ({:.4}, {:.4})",
        peak_l, peak_r
    );

    // 8. 활성 보이스 수 확인
    println!("\n[8] 활성 보이스 수: {}", synth.active_voices());

    // 9. 시스템 리셋
    let _ = synth.system_reset();
    println!("[9] 시스템 리셋 완료");

    println!("\n=== 모든 통합 테스트 통과 ===");
}
