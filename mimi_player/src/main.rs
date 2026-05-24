use cpal; // 오디오 스트림 타입을 인식하기 위해 cpal 크레이트 선언
use color_eyre::eyre::eyre;
use crossterm::event;
use mimi_core::{MimiCommand, MimiEngineHandle, PlayerState};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::{DefaultTerminal, Frame};
use std::path::PathBuf;
use std::sync::mpsc;

// 앱의 현재 화면 상태 정의
enum AppState {
    Browsing,
    Loading(f64), // 로딩 퍼센트 (0.0 ~ 1.0)
    Playing,
}

struct App {
    state: AppState,

    // 탐색기 관련
    file_list: Vec<PathBuf>,
    selected_index: usize,
    list_state: ListState,
    
    // 재생 관련
    song_name: String,
    player_state: PlayerState,
    song_time: String,
    engine: Option<MimiEngineHandle>,
    _stream: Option<cpal::Stream>,

    // 레벨미터
    channel_levels_a: [u8; 16],
    channel_levels_b: [u8; 16],

    // 비동기 로딩용 채널
    loading_rx: Option<mpsc::Receiver<Result<(MimiEngineHandle, cpal::Stream), String>>>,
}

impl App {
    fn new() -> Self {
        Self {
            state: AppState::Browsing,
            song_name: String::new(),
            player_state: PlayerState::Stopped,
            song_time: String::new(),
            file_list: Vec::new(),
            selected_index: 0,
            list_state: ListState::default(),
            engine: None,
            _stream: None,
            channel_levels_a: [0u8; 16],
            channel_levels_b: [0u8; 16],
            loading_rx: None,
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

    // assets 폴더 및 하위 디렉토리를 재귀적으로 탐색하여 미디 파일 스캔
    let mut stack = vec![PathBuf::from(assets_path)];

    // 루트 assets 디렉토리 존재 확인
    if !std::path::Path::new(assets_path).exists() {
        return Err(eyre!("Assets 디렉토리를 찾을 수 없음: {}", assets_path));
    }

    while let Some(current_dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // 디렉토리인 경우 스택에 추가하여 내부 탐색 수행
                    stack.push(path);
                } else if let Some(ext) = path.extension() {
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
    if !app.file_list.is_empty() {
        app.list_state.select(Some(0));
    }

    let mut needs_redraw = true;
    'main_loop: loop {
        if needs_redraw {
            terminal.draw(|f| render(f, &mut app))?;
            needs_redraw = false;
        }

        // 로딩 중일 때 비동기 결과 확인
        if let AppState::Loading(ref mut progress) = app.state {
            // 로딩 바 애니메이션 효과 (가짜 진행도)
            *progress = (*progress + 0.05).min(0.95);
            needs_redraw = true;

            if let Some(rx) = &app.loading_rx {
                if let Ok(result) = rx.try_recv() {
                    match result {
                        Ok((handle, stream)) => {
                            app.engine = Some(handle);
                            app._stream = Some(stream);
                            app.player_state = PlayerState::Playing;
                            app.state = AppState::Playing;
                        }
                        Err(_) => app.state = AppState::Browsing,
                    }
                    app.loading_rx = None;
                }
            }
        }

        if event::poll(std::time::Duration::from_millis(16))? {
            // MIMI PLAYER 제어 입력 처리
            if let event::Event::Key(key) = event::read()? {
                // 키를 누를 때만 이벤트를 처리하도록 제한 (Release 이벤트 무시)
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }

                // 로딩 중에는 모든 키 입력 차단
                if matches!(app.state, AppState::Loading(_)) {
                    continue;
                }

                match key.code {
                    event::KeyCode::Up | event::KeyCode::Down if matches!(app.state, AppState::Loading(_)) => {
                        continue;
                    }

                    event::KeyCode::Up => {
                        if app.selected_index > 0 {
                            app.selected_index -= 1;
                            app.list_state.select(Some(app.selected_index));
                            needs_redraw = true;
                        }
                    }
                    event::KeyCode::Down => {
                        if !app.file_list.is_empty() && app.selected_index < app.file_list.len() - 1 {
                            app.selected_index += 1;
                            app.list_state.select(Some(app.selected_index));
                            needs_redraw = true;
                        }
                    }
                    // 선택 및 재생
                    event::KeyCode::Enter => {
                        // 이미 재생중이면 무시
                        if matches!(app.state, AppState::Playing) {
                            continue;
                        }
                        if let Some(path) = app.file_list.get(app.selected_index) {
                            // 기존 엔진 정지 (Drop 시 스트림 멈춤)
                            app.engine = None;
                            app._stream = None;
                            
                            let (tx, rx) = mpsc::channel();
                            app.loading_rx = Some(rx);
                            app.state = AppState::Loading(0.0);
                            needs_redraw = true;

                            let path_clone = path.clone();
                            let sf_path_str = sf_path.to_string();

                            // 엔진 생성을 별도 스레드에서 수행
                            std::thread::spawn(move || {
                                let res = (|| -> Result<(MimiEngineHandle, cpal::Stream), String> {
                                    let midi_bytes = std::fs::read(&path_clone)
                                        .map_err(|e| format!("파일 읽기 실패: {:?}", e))?;
                                    let (handle, stream) = mimi_core::spawn_mimi_engine(&sf_path_str, midi_bytes)
                                        .map_err(|e| format!("엔진 생성 실패: {:?}", e))?;
                                    
                                    // 재생 시작 명령
                                    handle.send_command(MimiCommand::Play).ok();
                                    Ok((handle, stream))
                                })();
                                let _ = tx.send(res);
                            });

                            app.song_name = path.file_name().unwrap().to_string_lossy().to_string();
                        }
                    }
                    // 재생 제어
                    event::KeyCode::Char(' ') => {
                        if let Some(handle) = &app.engine {
                            match app.player_state {
                                PlayerState::Stopped | PlayerState::Paused => {
                                    if handle.send_command(MimiCommand::Play).is_ok() {
                                        app.player_state = PlayerState::Playing;
                                        needs_redraw = true;
                                    }
                                }
                                PlayerState::Playing => {
                                    if handle.send_command(MimiCommand::Pause).is_ok() {
                                        app.player_state = PlayerState::Paused;
                                        needs_redraw = true;
                                    }
                                }
                            }
                        }
                    }
                    event::KeyCode::Char('s') => {
                        if let Some(handle) = &app.engine {
                            if app.player_state != PlayerState::Stopped {
                                if handle.send_command(MimiCommand::Stop).is_ok() {
                                    app.player_state = PlayerState::Stopped;
                                    needs_redraw = true;
                                }
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
                        app.state = AppState::Browsing;
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
                    mimi_core::MidiEngineEvent::ChannelLevel { port, levels } => {
                        if port == 0 {
                            app.channel_levels_a = levels;
                        } else {
                            app.channel_levels_b = levels;
                        }
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }
        }

    }
    Ok(())
}

fn render(frame: &mut Frame, app: &mut App) {
    let version = env!("CARGO_PKG_VERSION");
    let block = Block::bordered()
        .title_top(Line::from(format!("MIMI PLAYER - {}", version)).centered())
        .title_bottom("↑/↓: Select | Enter: Play | Esc: Back")
        .title_bottom("<space>: Play/Pause | <s>: Stop")
        .title_bottom("<q> to Exit");

    let area = frame.area();

    match app.state {
        AppState::Browsing => {
            // 탐색기 모드
        let items: Vec<ListItem> = app.file_list.iter().map(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            ListItem::new(name.to_string())
        }).collect();

        let list = List::new(items)
            .block(block.title(" Select MIDI File ".bold()))
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD).bg(Color::DarkGray))
            .highlight_symbol("> ");
            
        frame.render_stateful_widget(list, area, &mut app.list_state);

        // 스크롤바 렌더링
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
            &mut ScrollbarState::new(app.file_list.len()).position(app.selected_index),
        );
        }
        AppState::Playing => {
        use ratatui::layout::{Constraint, Direction, Layout};

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(block.inner(area));

        frame.render_widget(block.title(" Playback Info ".bold()), area);

        let info_text = vec![
            Line::from(vec![
                Span::raw("Now Playing: "),
                Span::styled(&app.song_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(format!("State: {:?}  |  Time: {}", app.player_state, app.song_time)),
            Line::from(Span::styled("MIMI (MIDI Engine for Interactive Music & Instrumentation)", Style::default().fg(Color::Gray)))
        ];
        frame.render_widget(Paragraph::new(info_text), outer[0]);

        let meter_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer[2]);

        for (port_idx, (meter_area, levels)) in [
            (meter_layout[0], app.channel_levels_a),
            (meter_layout[1], app.channel_levels_b),
        ].iter().enumerate() {
            let port_label = if port_idx == 0 { "Port A" } else { "Port B" };
            let port_block = Block::bordered().title(format!(" {port_label} Channel Levels ").bold());
            let inner = port_block.inner(*meter_area);
            frame.render_widget(port_block, *meter_area);

            let bar_constraints: Vec<Constraint> = (0..16).map(|_| Constraint::Ratio(1, 16)).collect();
            let bar_areas = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(bar_constraints)
                .split(inner);

            for (ch, bar_area) in bar_areas.iter().enumerate() {
                let vel = levels[ch];
                let ratio = vel as f64 / 127.0;
                let color = if vel > 100 {
                    Color::Red
                } else if vel > 60 {
                    Color::Yellow
                } else {
                    Color::Green
                };

                // 채널 영역을 세로 VU 미터 + 레이블 행으로 분리
                let ch_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(1),
                        Constraint::Length(1),
                    ])
                    .split(*bar_area);

                // 미터 영역을 빈 공간(위) + 채워진 공간(아래)으로 분할
                let filled = (ratio * 100.0).round() as u16;
                let empty = 100u16.saturating_sub(filled);
                let bar_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(empty),
                        Constraint::Percentage(filled),
                    ])
                    .split(ch_layout[0]);

                // 위쪽 빈 공간
                frame.render_widget(
                    Block::default().style(Style::default().bg(Color::Black)),
                    bar_layout[0],
                );

                // 아래쪽 채워진 바
                frame.render_widget(
                    Block::default().style(Style::default().bg(color)),
                    bar_layout[1],
                );

                // 고정 레이블 행
                frame.render_widget(
                    Paragraph::new(format!("{:02}", ch + 1))
                        .style(Style::default().fg(Color::White))
                        .alignment(ratatui::layout::Alignment::Center),
                    ch_layout[1],
                );
            }
        }
    }
        AppState::Loading(progress) => {
            // 로딩 화면 및 로딩 바
            let center_area = centered_rect(60, 20, area);
            let loader = Gauge::default()
                .block(Block::bordered().title(" Loading Engine... ".bold()))
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
                .ratio(progress);
            
            frame.render_widget(loader, center_area);
        }
    }
}

// 로딩 바 배치를 위한 중앙 영역 계산 함수
fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)])
        .split(popup_layout[1])[1]
}
