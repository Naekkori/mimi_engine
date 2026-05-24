use cpal; // 오디오 스트림 타입을 인식하기 위해 cpal 크레이트 선언
use color_eyre::eyre::eyre;
use crossterm::event;
use mimi_core::{MimiCommand, MimiEngineHandle, PlayerState};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::path::PathBuf;

struct App {
    // 탐색기 관련
    file_list: Vec<PathBuf>,
    selected_index: usize,
    
    // 재생 관련
    song_name: String,
    player_state: PlayerState,
    song_time: String,
    engine: Option<MimiEngineHandle>,
    _stream: Option<cpal::Stream>, // 오디오 스트림 수명 유지를 위한 필드
}

impl App {
    fn new() -> Self {
        Self {
            song_name: String::new(),
            player_state: PlayerState::Stopped,
            song_time: String::new(),
            file_list: Vec::new(),
            selected_index: 0,
            engine: None,
            _stream: None,
        }
    }
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let app_result = run_app(&mut terminal, App::new());
    ratatui::restore();
    app_result?;
    Ok(())
}

fn run_app(terminal: &mut DefaultTerminal, mut app: App) -> color_eyre::Result<()> {
    let assets_path = "assets";
    let sf_path = "assets/soundfont.sf2";

    // assets 폴더에서 미디 파일 목록 읽기
    let entries = std::fs::read_dir(assets_path).map_err(|e| eyre!("Failed to read assets dir: {:?}", e))?;
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if ext_str == "mid" || ext_str == "midi" {
                        app.file_list.push(path);
                    }
                }
            }
        }
    }
    
    // 파일명 기준 정렬
    app.file_list.sort();

    'main_loop: loop {
        let mut needs_redraw = false;

        // 입력을 대기 (50ms = 약 20fps, CPU 부하 감소)
        if event::poll(std::time::Duration::from_millis(50))? {
            // MIMI PLAYER 제어 입력 처리
            if let event::Event::Key(key) = event::read()? {
                // 키를 누를 때만 이벤트를 처리하도록 제한 (Release 이벤트 무시)
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    // 위/아래 이동
                    event::KeyCode::Up => {
                        if app.selected_index > 0 {
                            app.selected_index -= 1;
                            needs_redraw = true;
                        }
                    }
                    event::KeyCode::Down => {
                        if !app.file_list.is_empty() && app.selected_index < app.file_list.len() - 1 {
                            app.selected_index += 1;
                            needs_redraw = true;
                        }
                    }
                    // 선택 및 재생
                    event::KeyCode::Enter => {
                        if let Some(path) = app.file_list.get(app.selected_index) {
                            // 기존 엔진 정지 (Drop 시 스트림 멈춤)
                            app.engine = None;
                            app._stream = None;
                            
                            let midi_bytes = std::fs::read(path).map_err(|e| eyre!("Can't read Midi: {:?}", e))?;
                            let (handle, stream) = mimi_core::spawn_mimi_engine(sf_path, midi_bytes)
                                .map_err(|e| eyre!("Failed to spawn engine: {:?}", e))?;
                            
                            app.song_name = path.file_name().unwrap().to_string_lossy().to_string();
                            handle.send_command(MimiCommand::Play).expect("Failed to play");
                            app.player_state = PlayerState::Playing;
                            app.engine = Some(handle);
                            app._stream = Some(stream);
                            needs_redraw = true;
                        }
                    }
                    // 재생 제어
                    event::KeyCode::Char(' ') => {
                        if let Some(handle) = &app.engine {
                            match app.player_state {
                                PlayerState::Stopped | PlayerState::Paused => {
                                    handle.send_command(MimiCommand::Play).expect("Failed to play");
                                    app.player_state = PlayerState::Playing;
                                    needs_redraw = true;
                                }
                                PlayerState::Playing => {
                                    handle.send_command(MimiCommand::Pause).expect("Failed to pause");
                                    app.player_state = PlayerState::Paused;
                                    needs_redraw = true;
                                }
                            }
                        }
                    }
                    event::KeyCode::Char('s') => {
                        if let Some(handle) = &app.engine {
                            if app.player_state != PlayerState::Stopped {
                                handle.send_command(MimiCommand::Stop).expect("Failed to stop");
                                app.player_state = PlayerState::Stopped;
                                needs_redraw = true;
                            }
                        }
                    }
                    // 리스트로 돌아가기 (선택 바 초기화 등은 안함)
                    event::KeyCode::Esc => {
                        if let Some(handle) = &app.engine {
                            handle.send_command(MimiCommand::Stop).ok();
                        }
                        app.engine = None;
                        app._stream = None;
                        app.song_name = "Ready.".to_string();
                        app.player_state = PlayerState::Stopped;
                        app.song_time = String::new();
                        needs_redraw = true;
                    }
                    event::KeyCode::Char('q') => break 'main_loop,
                    _ => {}
                }
            }
        }

        // 엔진 상태 업데이트
        if let Some(handle) = &app.engine {
            if let Ok(status) = handle.get_status() {
                if app.player_state != status.state {
                    app.player_state = status.state;
                    needs_redraw = true;
                }

                let seconds = status.current_time.as_secs();
                let new_time_str = format!(
                    "{:02}:{:02} ({:>5} / {:>5} tick)",
                    seconds / 60,
                    seconds % 60,
                    status.current_tick,
                    status.total_tick
                );

                if app.song_time != new_time_str {
                    app.song_time = new_time_str;
                    needs_redraw = true;
                }
            }
        }

        // 비동기 이벤트 처리
        if let Some(handle) = &app.engine {
            while let Ok(ui_event) = handle.ui_rx.try_recv() {
                match ui_event {
                    mimi_core::MidiEngineEvent::SmfKaraokeText { text } => {
                        app.song_name = text;
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }
        }

        if needs_redraw {
            terminal.draw(|f| render(f, &app))?;
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let version = env!("CARGO_PKG_VERSION");
    let block = Block::bordered()
        .title_top(Line::from(format!("MIMI PLAYER - {}", version)).centered())
        .title_bottom("↑/↓: Select | Enter: Play | Esc: Back")
        .title_bottom("<space>: Play/Pause | <s>: Stop")
        .title_bottom("<q> to Exit");

    let area = frame.area();

    if app.engine.is_none() {
        // 탐색기 모드
        let items: Vec<ListItem> = app.file_list.iter().enumerate().map(|(i, path)| {
            let name = path.file_name().unwrap().to_string_lossy();
            let style = if i == app.selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{} {}", if i == app.selected_index { ">" } else { " " }, name)).style(style)
        }).collect();

        let list = List::new(items)
            .block(block.title(" Select MIDI File ".bold()))
            .highlight_style(Style::default().bg(Color::DarkGray));
            
        frame.render_widget(list, area);
    } else {
        // 재생 모드
        let text = vec![
            Line::from(vec![
                Span::raw("Now Playing: "),
                Span::styled(&app.song_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(format!("Player State: {:?}", app.player_state)),
            Line::from(format!("Time: {}", app.song_time)),
            Line::from(""),
            Line::from(Span::styled("Press Esc to return to file list", Style::default().fg(Color::Gray))),
        ];
        let paragraph = Paragraph::new(text).block(block.title(" Playback Info ".bold()));
        frame.render_widget(paragraph, area);
    }
}
