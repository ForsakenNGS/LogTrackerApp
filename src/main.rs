#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod updater;

use eframe::egui;
use tinyfiledialogs::MessageBoxIcon;
use std::time::{SystemTime, Duration};
use updater::{Updater, UpdaterGuiData};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

struct LogTrackerApp {
    gui_data: Arc<Mutex<UpdaterGuiData>>,
    updater_arc: Arc<Mutex<Updater>>,
    updater_thread: Option<JoinHandle<()>>
}

impl LogTrackerApp {

    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self{
            gui_data: Arc::new(Mutex::new(UpdaterGuiData{
                ctx: None,
                ..Default::default()
            })),
            updater_arc: Arc::new(Mutex::new(Updater::new())),
            updater_thread: None
        }
    }

    pub fn start_update_thread(&mut self) {
        {
            let updater = &mut self.updater_arc.lock().unwrap();
            updater.set_gui_data(self.gui_data.clone());
            updater.load_config();
            updater.read_addon_data();
        }
        let updater_thread = self.updater_arc.clone();
        let gui_data  = self.gui_data.clone();
        self.updater_thread = Some(thread::spawn(move || {
            thread::sleep(Duration::new(1, 0));
            let mut last_rate_update = SystemTime::now() - Duration::new(30, 0);
            let mut last_export = SystemTime::now();
            let mut pause_until = SystemTime::now();
            loop {
                if !updater_thread.lock().unwrap().is_active() {
                    updater_thread.lock().unwrap().write_addon_data();
                    break;
                }
                updater_thread.lock().unwrap().update_addon();
                if !updater_thread.lock().unwrap().is_update_possible() {
                    {
                        let status_text = format!("Update completed.");
                        gui_data.lock().unwrap().status_text = status_text;
                        updater_thread.lock().unwrap().update_gui();
                    }
                    thread::sleep(Duration::new(1, 0));
                    continue;
                }
                if pause_until > SystemTime::now() {
                    thread::sleep(Duration::new(5, 0));
                    continue;
                }
                let last_rate_update_secs = SystemTime::now().duration_since(last_rate_update).unwrap().as_secs();
                if last_rate_update_secs > 30 {
                    updater_thread.lock().unwrap().update_api_limit();
                    last_rate_update = SystemTime::now();
                }
                let success = updater_thread.lock().unwrap().update_next();
                let last_export_secs = SystemTime::now().duration_since(last_export).unwrap().as_secs();
                if last_export_secs > 30 {
                    updater_thread.lock().unwrap().write_addon_data();
                    last_export = SystemTime::now();
                }
                if success {
                    thread::sleep(Duration::new(0, 10000));
                } else {
                    pause_until = SystemTime::now() + Duration::new(60, 0);
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
            let gui_data = &mut self.gui_data.lock().unwrap();
            if gui_data.ctx.is_none() {
                gui_data.ctx = Some(ctx.clone());
            }
            let panel_width = ui.available_width();
            ui.vertical(|ui| {
                ui.set_height( ui.available_height() - 15.0 );
                ui.label("Configuration");
                ui.group(|ui| {
                    ui.label("Game directory");
                    ui.horizontal(|ui| {
                        ui.add( 
                            egui::TextEdit::singleline(&mut gui_data.game_dir)
                                .desired_width(panel_width - 100.0).interactive(false)
                        );
                        if ui.button("Choose...").clicked() {
                            if let Some(path) = tinyfiledialogs::select_folder_dialog("Game directory", gui_data.game_dir.as_str()) {
                                let mut game_dir_wtf = PathBuf::from(path.clone());
                                game_dir_wtf.push("WTF");
                                if game_dir_wtf.is_dir() {
                                    let mut updater = self.updater_arc.lock().unwrap();
                                    updater.set_game_dir(&path);
                                    gui_data.game_dir = path;
                                } else {
                                    tinyfiledialogs::message_box_ok("Error", "Invalid game directory!", MessageBoxIcon::Error);
                                }
                            }
                        }
                    });
                    let label_api_id = ui.label("WCL API Client-ID");
                    let input_api_id = ui.add(
                        egui::TextEdit::singleline(&mut gui_data.api_id)
                            .desired_width(ui.available_width())
                    ).labelled_by(label_api_id.id);
                    if input_api_id.changed() {
                        let mut updater = self.updater_arc.lock().unwrap();
                        updater.set_api_id(&gui_data.api_id);
                    }
                    let label_api_secret = ui.label("WCL API Client-Secret");
                    let input_api_secret = ui.add(
                        egui::TextEdit::singleline(&mut gui_data.api_secret)
                            .desired_width(ui.available_width()).password(true)
                    ).labelled_by(label_api_secret.id);
                    if input_api_secret.changed() {
                        let mut updater = self.updater_arc.lock().unwrap();
                        updater.set_api_secret(&gui_data.api_secret);
                    }
                });
                ui.label("Manual update");
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let input_width = (panel_width - 95.0) / 2.0;
                        ui.vertical(|ui| {
                            ui.set_width(input_width);
                            let label_manual_realm = ui.label("Realm");
                            ui.text_edit_singleline(&mut gui_data.manual_realm).labelled_by(label_manual_realm.id);
                        });
                        ui.vertical(|ui| {
                            ui.set_width(input_width);
                            let label_manual_realm = ui.label("Player");
                            ui.text_edit_singleline(&mut gui_data.manual_player).labelled_by(label_manual_realm.id);
                        });
                        ui.vertical(|ui| {
                            ui.set_width(60.0);
                            ui.add_space(15.0);
                            if ui.button("Update").clicked() {
                                let mut updater = self.updater_arc.lock().unwrap();
                                let player = updater.get_player(&gui_data.manual_realm, &gui_data.manual_player).clone();
                                let success = updater.update_player(player);
                                if success {
                                    gui_data.manual_result = format!("Successfully updated {}-{}", gui_data.manual_player, gui_data.manual_realm);
                                } else {
                                    gui_data.manual_result = format!("Failed to update {}-{}", gui_data.manual_player, gui_data.manual_realm);
                                }
                            }
                        });
                    });
                    ui.vertical(|ui| {
                        let label_manual_result = ui.label("Result");
                        ui.add(
                            egui::TextEdit::singleline(&mut gui_data.manual_result)
                                .desired_width(ui.available_width()).interactive(false)
                        ).labelled_by(label_manual_result.id);
                    });
                });
            });
            ui.label(&gui_data.status_text);
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    // Initialize GUI
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(400.0, 300.0)),
        min_window_size: Some(egui::vec2(350.0, 300.0)),
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
