use std::process::exit;
use color_eyre::eyre::eyre;
use crossterm::event;
use mimi_core::{MimiCommand, PlayerState};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};

struct App {
    song_name: String,
    player_state: PlayerState,
}

impl App {
    fn new() -> Self {
        Self {
            song_name: String::new(),
            player_state: PlayerState::Stopped,
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
    //MIMI Engine 초기화
    // 에셋 경로 설정 및 미디 바이너리 로드
    let sf_path = "assets/soundfont.sf2";
    let midi_path = "assets/test.mid";
    app.song_name = "Reading MidiFile...".to_string();
    let midi_bytes =
        std::fs::read(midi_path).map_err(|e| eyre!("Can't read Midi File! {:?}", e))?;

    // 사운드폰트 파일 확인
    app.song_name = "Check SoundFont".to_string();
    if !std::path::Path::new(sf_path).exists() {
        return Err(eyre!("Soundfont doesn't exist at {:?}", sf_path));
    }

    //엔진 초기화
    app.song_name = "Initialize Engine...".to_string();
    let (_engine_handle, _stream) = mimi_core::spawn_mimi_engine(sf_path, midi_bytes)
        .map_err(|e| eyre!("Failed to spawn engine: {:?}", e))?;
    app.song_name = "Ready.".to_string();

    'main_loop: loop {
        terminal.draw(|f| render(f, &app))?;
        //MIMI PLAYER 제어 입력 처리
        if event::poll(std::time::Duration::from_millis(500))? {
            if let event::Event::Key(key) = event::read()? {
                // 키를 누를 때만 이벤트를 처리하도록 제한 (Release 이벤트 무시)
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    event::KeyCode::Char(' ') => {
                        match app.player_state {
                            PlayerState::Stopped | PlayerState::Paused => {
                                _engine_handle.send_command(MimiCommand::Play).expect("Failed to play");
                                app.player_state = PlayerState::Playing;
                            }
                            PlayerState::Playing => {
                                _engine_handle.send_command(MimiCommand::Pause).expect("Failed to pause");
                                app.player_state = PlayerState::Paused;
                            }
                        }
                    }
                    event::KeyCode::Char('s') => {
                        if app.player_state != PlayerState::Stopped {
                            _engine_handle.send_command(MimiCommand::Stop).expect("Failed to stop");
                            app.player_state = PlayerState::Stopped;
                        }
                    }
                    event::KeyCode::Char('q') => break 'main_loop,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let version = env!("CARGO_PKG_VERSION");
    let block = Block::bordered()
        .title_top(Line::from(format!("MIMI PLAYER - {}", version)).centered())
        .title_bottom("<space> to Play/Pause")
        .title_bottom("<s> to Stop")
        .title_bottom("<q> to Exit");

    let text = vec![
        Line::from(format!("{}", app.song_name)),
        Line::from(format!("Player State: {:?}", app.player_state)),
    ];
    let paragraph = Paragraph::new(text).block(block);

    frame.render_widget(paragraph, frame.area());
}
