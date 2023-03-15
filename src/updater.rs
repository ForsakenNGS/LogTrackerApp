extern crate serde;
extern crate serde_json;

use serde::{Serialize, Deserialize};
use std::env::var;
use std::fs::{self, File};
use std::io::{Read, Write, Result};

#[derive(Serialize, Deserialize)]
pub struct UpdaterConfig {
    game_dir: Box<str>,
    api_key: Box<str>,
    api_secret: Box<str>
}

pub struct Updater {
    config: UpdaterConfig
}

impl Updater {
    pub fn new() -> Updater {
        Updater{
            config: UpdaterConfig{
                game_dir: "".into(),
                api_key: "".into(),
                api_secret: "".into()
            }
        }
    }

    pub fn get_game_dir(&self) -> &str {
        &self.config.game_dir
    }

    pub fn get_api_key(&self) -> &str {
        &self.config.api_key
    }

    pub fn get_api_secret(&self) -> &str {
        &self.config.api_secret
    }

    pub fn set_game_dir(&mut self, game_dir: &str) {
        self.config.game_dir = game_dir.clone().into();
        self.write_config();
    }

    pub fn set_api_key(&mut self, api_key: &str) {
        self.config.api_key = api_key.clone().into();
        self.write_config();
    }

    pub fn set_api_secret(&mut self, api_secret: &str) {
        self.config.api_secret = api_secret.clone().into();
        self.write_config();
    }

    pub fn load_config(&mut self) {
        let config_home = var("XDG_CONFIG_HOME")
            .or_else(|_| var("HOME").map(|home| format!("{}/.logtrackerapp", home)))
            .unwrap();
        let config_meta = r#fs::metadata(config_home.to_owned());
        if config_meta.is_ok() && config_meta.unwrap().is_file() {
            let data = fs::read_to_string(config_home).unwrap();
            self.config = serde_json::from_str(data.as_str()).unwrap();
        }
    }

    pub fn write_config(&self) {
        let config_home = var("XDG_CONFIG_HOME")
            .or_else(|_| var("HOME").map(|home| format!("{}/.logtrackerapp", home)))
            .unwrap();
        let mut file = File::create(config_home).unwrap();
        let data = serde_json::to_string(&self.config);
        file.write_all(data.unwrap().as_bytes())
            .expect("Failed to write configuration");
    }

}