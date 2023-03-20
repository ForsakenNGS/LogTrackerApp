mod updater;

use eframe::egui;
use tinyfiledialogs::MessageBoxIcon;
use std::time::{SystemTime, Duration};
use updater::Updater;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

struct LogTrackerApp {
    game_dir: String,
    api_id: String,
    api_secret: String,
    status_text: Arc<Mutex<String>>,
    updater_arc: Arc<Mutex<Updater>>,
    updater_thread: Option<JoinHandle<()>>
}

impl LogTrackerApp {

    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut updater = Updater::new();
        updater.load_config();
        updater.read_addon_data();
        Self{
            game_dir: updater.get_game_dir().to_owned(),
            api_id: updater.get_api_key().to_owned(),
            api_secret: updater.get_api_secret().to_owned(),
            status_text: Arc::new(Mutex::new("".to_owned())),
            updater_arc: Arc::new(Mutex::new(updater)),
            updater_thread: None
        }
    }

    pub fn start_update_thread(&mut self) {
        let updater_thread = self.updater_arc.clone();
        let status_text_thread  = self.status_text.clone();
        self.updater_thread = Some(thread::spawn(move || {
            let mut last_status_update = SystemTime::now();
            let mut last_export = SystemTime::now();
            let mut pause_until = SystemTime::now();
            loop {
                if !updater_thread.lock().unwrap().is_active() {
                    updater_thread.lock().unwrap().write_addon_data();
                    break;
                }
                if pause_until > SystemTime::now() {
                    continue;
                }
                updater_thread.lock().unwrap().update_addon();
                let (update_pos, update_max, pause) = updater_thread.lock().unwrap().update_next();
                let last_status_update_secs = SystemTime::now().duration_since(last_status_update).unwrap().as_secs();
                if last_status_update_secs > 2 {
                    let status_text = format!("Updated {} / {}", update_pos, update_max);
                    *status_text_thread.lock().unwrap() = status_text;
                    last_status_update = SystemTime::now();
                }
                let last_export_secs = SystemTime::now().duration_since(last_export).unwrap().as_secs();
                if last_export_secs > 30 {
                    updater_thread.lock().unwrap().write_addon_data();
                    last_export = SystemTime::now();
                }
                if pause {
                    pause_until = SystemTime::now() + Duration::new(300, 0);
                }
            }
        }));
    }

}

impl eframe::App for LogTrackerApp {

    fn on_close_event(&mut self) -> bool {
        self.updater_arc.lock().unwrap().stop();
        if let Some(updater_thread) = self.updater_thread.take() {
            updater_thread.join().unwrap();
        }
        true
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.set_min_width(300.0);
            ui.set_min_height(200.0);
            ui.vertical(|ui| {
                ui.label("Game directory");
                ui.horizontal(|ui| {
                    ui.add( 
                        egui::TextEdit::singleline(&mut self.game_dir)
                            .desired_width(ui.available_width() - 100.0).interactive(false)
                    );
                    //ui.input(reader)
                    if ui.button("Choose...").clicked() {
                        if let Some(path) = tinyfiledialogs::select_folder_dialog("Game directory", self.game_dir.as_str()) {
                            let mut game_dir_wtf = PathBuf::from(path.clone());
                            game_dir_wtf.push("WTF");
                            if game_dir_wtf.is_dir() {
                                let mut updater = self.updater_arc.lock().unwrap();
                                updater.set_game_dir(&path);
                            } else {
                                tinyfiledialogs::message_box_ok("Error", "Invalid game directory!", MessageBoxIcon::Error);
                            }
                        }
                    }
                });
                let label_api_id = ui.label("WCL API Client-ID");
                let input_api_id = ui.add(
                    egui::TextEdit::singleline(&mut self.api_id)
                        .desired_width(ui.available_width())
                ).labelled_by(label_api_id.id);
                if input_api_id.changed() {
                    let mut updater = self.updater_arc.lock().unwrap();
                    updater.set_api_key(&self.api_id);
                }
                let label_api_secret = ui.label("WCL API Client-Secret");
                let input_api_secret = ui.add(
                    egui::TextEdit::singleline(&mut self.api_secret)
                        .desired_width(ui.available_width()).password(true)
                ).labelled_by(label_api_secret.id);
                if input_api_secret.changed() {
                    let mut updater = self.updater_arc.lock().unwrap();
                    updater.set_api_secret(&self.api_secret);
                }
                ui.set_height( ui.available_height() - 15.0 );
            });
            ui.label(self.status_text.lock().unwrap().to_owned());
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    // Initialize GUI
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(400.0, 300.0)),
        min_window_size: Some(egui::vec2(350.0, 250.0)),
        ..Default::default()
    };
    eframe::run_native(
        "LogTracker App", options,
        Box::new(|cc| {
            let mut app = LogTrackerApp::new(cc);
            app.start_update_thread();
            Box::new(app)
        })
    )
}
