mod updater;

use gtk::{prelude::*, MessageDialog, Window, DialogFlags, MessageType, ButtonsType, glib};
use gtk::{Application, ApplicationWindow, Builder, FileChooserButton, Entry, Statusbar};
use std::time::SystemTime;
use updater::Updater;
use std::sync::{Arc, Mutex};
use std::path::Path;
use std::thread;
enum Message {
    UpdateStatus(String),
}

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
    let window_delete_updater = updater_arc.clone();
    // Initialize saved values
    {
        let mut updater = updater_arc.lock().unwrap();
        updater.load_config();
        updater.read_addon_data();
        let game_dir_str = updater.get_game_dir();
        file_picker.set_current_folder(Path::new(game_dir_str));
        api_key_entry.set_text(updater.get_api_key());
        api_secret_entry.set_text(updater.get_api_secret());
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
    window.connect_delete_event(move |_, _| {
        window_delete_updater.lock().unwrap().stop();
        Inhibit(false)
    });
    window.set_application(Some(application));
    window.show_all();
    // Start updater thread
    let (sender, receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    let updater_thread = updater_arc.clone();
    let status_thread = Arc::new(Mutex::new(status));
    let _bg_thread = thread::spawn(move || {
        let mut last_status_update = SystemTime::now();
        let mut last_export = SystemTime::now();
        loop {
            if !updater_thread.lock().unwrap().is_active() {
                updater_thread.lock().unwrap().write_addon_data();
                break;
            }
            updater_thread.lock().unwrap().update_addon();
            let (update_pos, update_max) = updater_thread.lock().unwrap().update_next();
            let last_status_update_secs = SystemTime::now().duration_since(last_status_update).unwrap().as_secs();
            if last_status_update_secs > 2 {
                let status_text = format!("Updated {} / {}", update_pos, update_max);
                let _result = sender.send(Message::UpdateStatus(status_text));
                last_status_update = SystemTime::now();
            }
            let last_export_secs = SystemTime::now().duration_since(last_export).unwrap().as_secs();
            if last_export_secs > 30 {
                updater_thread.lock().unwrap().write_addon_data();
                last_export = SystemTime::now();
            }
        }
    });
    receiver.attach(None, move |msg| {
        match msg {
            Message::UpdateStatus(text) => {
                let context_id = status_thread.lock().unwrap().context_id("Update progress");
                status_thread.lock().unwrap().push(context_id, text.as_str());
            }
        }
        glib::Continue(true)
    });
}

fn main() {
    env_logger::init();
    let app = Application::builder()
        .application_id("org.forsaken.LogTracker")
        .build();

    app.connect_activate(build_gui);

    app.run();
}
