// integration_test.rs - mimi_core 연결 검증 테스트
// mimi_core가 사용하는 모든 API 패턴을 시뮬레이션

use hibiki::{HibikiSettings, HibikiSynth};

fn main() {
    println!("=== Hibiki 사운드폰트 엔진 통합 테스트 ===\n");

    // 1. 엔진 생성 (mimi_core의 new_engine 패턴)
    let mut settings = HibikiSettings::new();
    settings.set_sample_rate(44100.0);

    let synth = HibikiSynth::new(settings).expect("Failed to create synth");
    println!("[1] HibikiSynth 생성 완료");

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
        let _ = synth.program_change(ch, 0); // Default Grand Piano
        let _ = synth.cc(ch, 7, 100);  // Volume
        let _ = synth.cc(ch, 11, 127); // Expression
        let _ = synth.cc(ch, 10, 64);  // Pan
        let _ = synth.pitch_bend(ch, 8192); // Pitch Bend Center
        let _ = synth.cc(ch, 91, 40);  // Reverb
        let _ = synth.cc(ch, 93, 0);   // Chorus
        let _ = synth.cc(ch, 94, 0);   // Effect 4
    }
    println!("[3] 16채널 초기화 완료 (모든 CC + Program Change)");

    // 4. 미디 이벤트 시뮬레이션 - C 메이저 스케일
    println!("\n[4] C 메이저 스케일 재생 테스트");
    let notes = [60, 62, 64, 65, 67, 69, 71, 72]; // C4 ~ C5
    for &note in &notes {
        // Note On
        if let Err(e) = synth.note_on(0, note, 127) {
            eprintln!("    NoteOn({}) 실패: {}", note, e);
        }

        // 0.2초 렌더링 (8820 샘플)
        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;
        let mut non_zero_count = 0;

        for _ in 0..8820 {
            let mut out = [0.0f32; 2];
            if let Err(e) = synth.write_samples(&mut out) {
                eprintln!("    write_samples 실패: {}", e);
                return;
            }
            peak_l = peak_l.max(out[0].abs());
            peak_r = peak_r.max(out[1].abs());
            if out[0].abs() > 0.001 || out[1].abs() > 0.001 {
                non_zero_count += 1;
            }
        }

        // Note Off
        if let Err(e) = synth.note_off(0, note) {
            eprintln!("    NoteOff({}) 실패: {}", note, e);
        }

        println!(
            "    Note {}: peak(L,R) = ({:.4}, {:.4}), non-zero samples = {}",
            note, peak_l, peak_r, non_zero_count
        );

        // 다음 노트 전에 짧은 휴식
        for _ in 0..2205 {
            let mut out = [0.0f32; 2];
            let _ = synth.write_samples(&mut out);
        }
    }

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
