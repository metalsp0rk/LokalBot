//! LokalBot for Linux and Windows.

#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod pages;
mod shell;
mod state;
mod theme;
mod ui;

use gpui_kit::component::TitleBar;
use gpui_kit::{
    App, AppContext as _, Bounds, Entity, KeyBinding, Size, Window, WindowBounds, WindowKind, WindowOptions, actions,
    px, size,
};
use lokalbot_core::{Library, LibraryPaths, Settings};

use crate::state::{AppData, AppEvent, AppHandle, Section};

actions!(lokalbot, [Quit, ToggleRecording]);

/// Command-line options. Most users never pass any; they exist for tests,
/// screenshots and a second library.
#[derive(Default)]
struct Options {
    storage_root: Option<std::path::PathBuf>,
    section: Option<Section>,
}

fn parse_options() -> Options {
    let args: Vec<String> = std::env::args().collect();
    let mut options = Options { storage_root: LibraryPaths::root_from_args(&args), ..Default::default() };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--page" {
            options.section = iter.next().and_then(|name| section_named(name));
        }
    }
    options
}

fn section_named(name: &str) -> Option<Section> {
    Some(match name.to_ascii_lowercase().as_str() {
        "today" => Section::Today,
        "meetings" => Section::Meetings,
        "timeline" => Section::Timeline,
        "people" => Section::People,
        "projects" => Section::Projects,
        "ask" => Section::Ask,
        "agent" => Section::Agent,
        "settings" => Section::Settings,
        _ => return None,
    })
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = parse_options();
    let paths = LibraryPaths::resolve(options.storage_root.as_deref());
    let library = match Library::open(paths.clone()) {
        Ok(library) => library,
        Err(err) => {
            eprintln!("LokalBot could not open its library at {}: {err:#}", paths.root().display());
            std::process::exit(1);
        }
    };
    let settings = Settings::load(&paths.settings_file());
    log::info!("library: {}", paths.root().display());

    gpui_kit::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        gpui_kit::init(cx);
        assets::load_fonts(cx);
        cx.set_app_identity("com.lokalbot.LokalBot", "LokalBot");

        let data = cx.new(|_| AppData::new(library, settings));
        if let Some(section) = options.section {
            data.update(cx, |data, _| data.section = section);
        }
        cx.set_global(AppHandle(data.clone()));
        bind_actions(cx);
        open_main_window(data.clone(), cx);
        data.update(cx, |data, cx| data.reload(cx));
    });
}

fn bind_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-shift-r", ToggleRecording, None),
    ]);
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_action(|_: &ToggleRecording, cx: &mut App| {
        AppData::global(cx).update(cx, |data, cx| data.toggle_recording(cx));
    });
}

fn open_main_window(data: Entity<AppData>, cx: &mut App) {
    let mut window_size = size(px(1280.), px(820.));
    if let Some(display) = cx.primary_display() {
        let display_size = display.bounds().size;
        window_size.width = window_size.width.min(display_size.width * 0.92);
        window_size.height = window_size.height.min(display_size.height * 0.92);
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
        window_min_size: Some(Size { width: px(960.), height: px(640.) }),
        kind: WindowKind::Normal,
        app_id: Some("com.lokalbot.LokalBot".into()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui_kit::WindowDecorations::Client),
        ..TitleBar::window_options()
    };
    let result = gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title("LokalBot");
        apply_appearance(&data, window, cx);
        let observed = data.clone();
        window
            .observe_window_appearance(move |window, cx| apply_appearance(&observed, window, cx))
            .detach();
        let subscribed = data.clone();
        window
            .subscribe(&data, cx, move |_, event: &AppEvent, window, cx| handle_event(&subscribed, event, window, cx))
            .detach();
        cx.new(|cx| shell::Workspace::new(window, cx))
    });
    if let Err(err) = result {
        log::error!("could not open the main window: {err:#}");
        cx.quit();
    }
}

fn apply_appearance(data: &Entity<AppData>, window: &mut Window, cx: &mut App) {
    let settings = data.read(cx).settings.clone();
    let dark = theme::is_dark(settings.app_theme, window.appearance());
    theme::apply(dark, settings.text_size.scale(), Some(window), cx);
}

fn handle_event(data: &Entity<AppData>, event: &AppEvent, window: &mut Window, cx: &mut App) {
    match event {
        AppEvent::AppearanceChanged => apply_appearance(data, window, cx),
        AppEvent::StartRecording | AppEvent::StopRecording => {
            data.update(cx, |data, cx| data.show_message("Recording is not connected yet in this build.", cx));
        }
        AppEvent::SettingsChanged => {}
    }
}
