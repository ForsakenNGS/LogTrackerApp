mod updater;

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Builder, FileChooserButton, Entry};
use updater::Updater;
use std::sync::{Arc, Mutex};
use std::path::Path;

fn build_gui(application: &Application) {
    let updater_arc = Arc::new(Mutex::new(Updater::new()));
    // Initialize GUI
    let glade_file = include_str!("../gui/log_tracker_app.glade");
    let builder = Builder::from_string(glade_file);
    // Get relevant GUI-Elements
    let window: ApplicationWindow = builder.object::<ApplicationWindow>("main_window")
        .expect("Unable to initialize main window!");
    let file_picker: FileChooserButton = builder.object::<FileChooserButton>("game_dir_picker").unwrap();
    let file_picker_updater = updater_arc.clone();
    let api_key_entry: Entry = builder.object::<Entry>("api_key").unwrap();
    let api_key_entry_updater = updater_arc.clone();
    let api_secret_entry: Entry = builder.object::<Entry>("api_secret").unwrap();
    let api_secret_entry_updater = updater_arc.clone();
    // Initialize saved values
    let mut updater = updater_arc.lock().unwrap();
    updater.load_config();
    let game_dir_str = updater.get_game_dir();
    file_picker.set_current_folder(Path::new(game_dir_str));
    api_key_entry.set_text(updater.get_api_key());
    api_secret_entry.set_text(updater.get_api_secret());
    // Connect relevant signals
    file_picker.connect_file_set(move |file_picker: &FileChooserButton| {
        let game_dir = file_picker.filename().expect("No directory chosen!");
        let game_dir_str = game_dir.to_str().unwrap().clone();
        file_picker_updater.lock().unwrap().set_game_dir(&game_dir_str);
        println!("Game dir picked! {}", game_dir_str);
    });
    api_key_entry.connect_changed(move |api_key_entry: &Entry| {
        let api_key = api_key_entry.text();
        let api_key_str = api_key.as_str();
        api_key_entry_updater.lock().unwrap().set_api_key(&api_key_str);
    });
    api_secret_entry.connect_changed(move |api_key_entry: &Entry| {
        let api_key = api_key_entry.text();
        let api_key_str = api_key.as_str();
        api_secret_entry_updater.lock().unwrap().set_api_secret(&api_key_str);
    });
    window.set_application(Some(application));
    window.show_all();
}

fn main() {
    let app = Application::builder()
        .application_id("org.forsaken.LogTracker")
        .build();

    app.connect_activate(build_gui);

    app.run();
}
