extern crate serde;
extern crate serde_json;

use std::env::var;
use std::path::PathBuf;
use std::fs::{self, File};
use std::io::Write;
use mlua::prelude::*;
use mlua::Table;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use graphql_client::{GraphQLQuery, Response};
use std::time::{Duration, SystemTime};
use std::thread::sleep;

#[derive(Serialize, Deserialize, Default)]
pub struct UpdaterConfig {
    game_dir: Box<str>,
    api_key: Box<str>,
    api_secret: Box<str>
}

#[derive(Clone)]
pub struct UpdaterPlayer {
    realm: Box<str>,
    name: Box<str>,
    pending: bool,
    last_update: u64,
    last_update_logs: u64
}

pub struct Updater {
    config: UpdaterConfig,
    players: HashMap<String, HashMap<String, UpdaterPlayer>>,
    update_queue: Vec<UpdaterPlayer>,
    update_queue_pos: usize
}

impl Updater {
    pub fn new() -> Updater {
        Updater{
            config: Default::default(),
            players: HashMap::new(),
            update_queue: Vec::new(),
            update_queue_pos: 0
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

    pub fn get_player(&mut self, realm: &String, player_name: &String) -> &mut UpdaterPlayer {
        let realm_players = self.players.entry(realm.clone()).or_insert_with(|| HashMap::new());
        realm_players.entry(player_name.clone()).or_insert_with(|| {
            UpdaterPlayer{
                realm: realm.as_str().into(), name: player_name.as_str().into(), 
                pending: false, last_update: 0, last_update_logs: 0
            }
        })
    }

    pub fn set_game_dir(&mut self, game_dir: &str) {
        self.config.game_dir = game_dir.into();
        self.write_config();
    }

    pub fn set_api_key(&mut self, api_key: &str) {
        self.config.api_key = api_key.into();
        self.write_config();
    }

    pub fn set_api_secret(&mut self, api_secret: &str) {
        self.config.api_secret = api_secret.into();
        self.write_config();
    }

    pub fn read_addon_data(&mut self) {
        let game_dir_str = String::from(self.config.game_dir.clone());
        let game_dir = PathBuf::from(game_dir_str);
        let mut game_wtf_accounts = PathBuf::from(game_dir.clone());
        game_wtf_accounts.push("WTF");
        game_wtf_accounts.push("Account");
        if !game_wtf_accounts.is_dir() {
            return
        }
        for account_dir in game_wtf_accounts.read_dir().expect("Failed to read WoW-Accounts!") {
            if let Ok(account_dir) = account_dir {
                let mut addon_lua_saved = PathBuf::from(account_dir.path());
                addon_lua_saved.push("SavedVariables");
                addon_lua_saved.push("LogTracker.lua");
                if addon_lua_saved.is_file() {
                    let lua = Lua::new();
                    let data_raw = fs::read_to_string(addon_lua_saved).unwrap();
                    if !lua.load(data_raw.as_str()).exec().is_err() {
                        let data: Table = lua.globals().get("LogTrackerDB").unwrap();
                        let data_realms: Table = data.get("playerData").unwrap();
                        for pair_realm in data_realms.pairs::<String, Table>() {
                            let (realm_name, player_list) = pair_realm.unwrap();
                            for pair_player in player_list.pairs::<String, Table>() {
                                let (player_name, player_details) = pair_player.unwrap();
                                let player_updated: u64 = player_details.get("lastUpdate").unwrap();
                                let player_updated_logs: u64 = player_details.get("lastUpdateLogs").or_else(|_| Ok::<u64, u64>(0)).unwrap();
                                let mut player = &mut self.get_player(&realm_name, &player_name);
                                player.last_update = player_updated;
                                player.last_update_logs = player_updated_logs;
                            }
                        }
                    }
                }
            }
        }
        let mut addon_lua_import = PathBuf::from(game_dir);
        addon_lua_import.push("Interface");
        addon_lua_import.push("AddOns");
        addon_lua_import.push("LogTracker");
        addon_lua_import.push("AppData.lua");
        if addon_lua_import.is_file() {
            let lua = Lua::new();
            let data_raw = fs::read_to_string(addon_lua_import).unwrap();
            if !lua.load(data_raw.as_str()).exec().is_err() {
                // TODO: Load previously exported entries
            }
        }
        self.rewrite_update_queue();
    }

    pub fn rewrite_update_queue(&mut self) {
        self.update_queue.clear();
        for pair_realm in self.players.iter() {
            let (_realm_name, player_list) = pair_realm;
            for pair_player in player_list.iter() {
                let (_player_name, player_details) = pair_player;
                self.update_queue.push(player_details.clone());
            }
        }
        self.update_queue.sort_by(|a, b| {
            if a.last_update_logs == b.last_update_logs {
                b.last_update.cmp(&a.last_update)
            } else {
                a.last_update_logs.cmp(&b.last_update_logs)
            }
        });
    }

    pub fn load_config(&mut self) {
        let config_home = var("XDG_CONFIG_HOME")
            .or_else(|_| var("HOME").map(|home| format!("{}/.logtrackerapp", home)))
            .unwrap();
        let config_meta = fs::metadata(config_home.to_owned());
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

    pub fn update_next(&mut self) -> (usize,usize) {
        let update_index = self.update_queue_pos;
        let update_count = self.update_queue.len();
        if update_index >= update_count {
            sleep(Duration::new(1, 0));
            return (update_index, update_count);
        }
        self.update_queue_pos += 1;
        let player = self.update_queue.get_mut(update_index).unwrap();
        player.last_update_logs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        //let client = UnionQuery::build_query();
        // TODO: Query updated logs
        return (update_index+1, update_count);
    }

}