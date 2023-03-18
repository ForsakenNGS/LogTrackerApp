mod updater;

use gtk::{prelude::*, MessageDialog, Window, DialogFlags, MessageType, ButtonsType};
use gtk::{Application, ApplicationWindow, Builder, FileChooserButton, Entry, Statusbar};
use updater::Updater;
use std::sync::{Arc, Mutex};
use std::path::Path;
use std::thread;

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
    let status: Statusbar = builder.object::<Statusbar>("status").unwrap();
    // Initialize saved values
    {
        let mut updater = updater_arc.lock().unwrap();
        updater.load_config();
        updater.read_addon_data();
        let game_dir_str = updater.get_game_dir();
        file_picker.set_current_folder(Path::new(game_dir_str));
        api_key_entry.set_text(updater.get_api_key());
        api_secret_entry.set_text(updater.get_api_secret());
        let (update_pos, update_max) = updater.update_next();
        println!("Updated {} / {}", update_pos, update_max);
    }
    // Connect relevant signals
    file_picker.connect_file_set(move |file_picker: &FileChooserButton| {
        let game_dir = file_picker.filename().expect("No directory chosen!");
        let game_dir_str = game_dir.to_str().unwrap().clone();
        let mut game_dir_wtf = game_dir.clone();
        game_dir_wtf.push("WTF");
        if game_dir_wtf.is_dir() {
            file_picker_updater.lock().unwrap().set_game_dir(&game_dir_str);
        } else {
            let error_dialog = MessageDialog::new(
                None::<&Window>, DialogFlags::empty(), MessageType::Error, ButtonsType::Close, 
                "Invalid game directory!"
            );
            error_dialog.run();
            error_dialog.close();
        }
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
    // Start updater thread
    let updater_thread = updater_arc.clone();
    let _bg_thread = thread::spawn(move || {
        loop {
            let (update_pos, update_max) = updater_thread.lock().unwrap().update_next();
            println!("Updated {} / {}", update_pos, update_max);
            /*
            let context_id = status.context_id("Update progress");
            let status_text = format!("Updated {} / {}", update_pos, update_max);
            status.push(context_id, status_text.as_str());
            */
        }
    });
}

fn main() {
    let app = Application::builder()
        .application_id("org.forsaken.LogTracker")
        .build();

    app.connect_activate(build_gui);

    app.run();
}
