extern crate serde;
extern crate serde_json;

use std::path::PathBuf;
use std::fs::{self, File};
use std::io::Write;
use std::sync::{Mutex, Arc};
use std::time::{Duration, SystemTime};
use std::thread::sleep;
use std::collections::HashMap;
use chrono::offset::Local;
use chrono::DateTime;
use eframe::egui;
use log::{info, warn};
use mlua::prelude::*;
use mlua::Table;
use serde::{Serialize, Deserialize};
use reqwest::{self, blocking::Client};
use oauth2::{AuthUrl,ClientId,ClientSecret,TokenResponse,TokenUrl, StandardTokenResponse, EmptyExtraTokenFields};
use oauth2::basic::{BasicClient, BasicTokenType};
use oauth2::reqwest::http_client;
use graphql_client::{reqwest::post_graphql_blocking as post_graphql, GraphQLQuery};

const UPDATE_INTERVAL_FAST: i64 = 86400;
const UPDATE_INTERVAL_SLOW: i64 = 604800;

#[derive(Serialize, Deserialize, Default)]
pub struct UpdaterConfig {
    game_dir: Box<str>,
    api_id: Box<str>,
    api_secret: Box<str>
}

#[derive(Clone, Default)]
pub struct UpdaterGuiData {
    pub ctx: Option<egui::Context>,
    pub game_dir: String,
    pub api_id: String,
    pub api_secret: String,
    pub manual_realm: String,
    pub manual_player: String,
    pub manual_result: String,
    pub status_text: String
}

#[derive(Clone, Default)]
pub struct UpdaterPlayer {
    realm: Box<str>,
    name: Box<str>,
    faction: Box<str>,
    class: i64,
    level: i64,
    ranking: HashMap<String, UpdaterRanking>,
    last_update: i64,
    last_update_logs: i64,
    last_update_addon: i64,
    update_priority: i64
}


#[derive(Clone, Default)]
pub struct UpdaterBaseData {
    classes: HashMap<String, UpdaterBaseDataClass>,
    region_by_server_slug: HashMap<String, String>
}


#[derive(Clone, Default)]
pub struct UpdaterBaseDataClass {
    id: i64,
    name: Box<str>,
    slug: Box<str>,
    specs: HashMap<String, UpdaterBaseDataClassSpec>
}

#[derive(Clone, Default)]
pub struct UpdaterBaseDataClassSpec {
    id: i64,
    name: Box<str>,
    slug: Box<str>,
    metric: Box<str>,
}

#[derive(Clone, Default)]
pub struct UpdaterRanking {
    encounters: i64,
    encounters_killed: i64,
    allstar_ratings: Vec<(i64,i64,i64)>,
    encounter_ratings: Vec<(i64,i64,i64)>
}

impl UpdaterRanking {
    pub fn clear(&mut self) {
        self.encounters = 0;
        self.encounters_killed = 0;
        self.allstar_ratings.clear();
        self.encounter_ratings.clear();
    }
    pub fn update_from_json(&mut self, data: &serde_json::Value, spec_id: i64) {
        // Fill with new data
        if let Some(best) = data.get("bestPerformanceAverage") {
            if !best.is_null() {
                self.allstar_ratings.push((
                    spec_id, 
                    best.as_f64().unwrap().round() as i64,
                    data.get("medianPerformanceAverage").unwrap().as_f64().unwrap().round() as i64
                ));
            }
        }
        if let Some(encounters) = data.get("rankings") {
            if !encounters.is_null() {
                let encounters = encounters.as_array().unwrap();
                // Clear encounter stats
                self.encounters = 0;
                self.encounters_killed = 0;
                let mut encounter_index = 0;
                for encounter_rank in encounters.iter() {
                    if self.encounter_ratings.len() <= encounter_index {
                        self.encounter_ratings.push((0 as i64, 0 as i64, 0 as i64));
                    }
                    let encounter_rating = self.encounter_ratings.get_mut(encounter_index).unwrap();
                    self.encounters += 1;
                    let spec_raw = encounter_rank.get("spec").unwrap();
                    if !spec_raw.is_null() {
                        let best = encounter_rank.get("rankPercent").unwrap().as_f64().unwrap().round() as i64;
                        let avg = encounter_rank.get("medianPercent").unwrap().as_f64().unwrap().round() as i64;
                        if best > encounter_rating.1 {
                            encounter_rating.0 = spec_id;
                            encounter_rating.1 = best;
                            encounter_rating.2 = avg;
                        }
                    }
                    if encounter_rating.1 > 0 {
                        self.encounters_killed += 1;
                    }
                    encounter_index += 1;
                }
            }
        }
    }
    pub fn update_from_lua(&mut self, data: Table) {
        // Clear values
        self.allstar_ratings.clear();
        self.encounter_ratings.clear();
        // Update from lua
        self.encounters = data.get(1).unwrap();
        self.encounters_killed = data.get(2).unwrap();
        let data_allstars: Table = data.get(3).unwrap();
        for pair_allstar in data_allstars.pairs::<String, Table>() {
            let (_allstar_index, allstar_details) = pair_allstar.unwrap();
            self.allstar_ratings.push((allstar_details.get(1).unwrap(), allstar_details.get(2).unwrap(), allstar_details.get(3).unwrap_or_default()));
        }
        let data_encounters_str: String = data.get(4).unwrap();
        let data_encounters: Vec<&str> = data_encounters_str.split("|").collect();
        for data_encounter in data_encounters.iter() {
            let data_encounter = data_encounter.to_string();
            let data_ratings: Vec<&str> = data_encounter.split(",").collect();
            self.encounter_ratings.push((
                data_ratings.get(0).unwrap().parse::<i64>().or_else(|_| Ok::<i64, i64>(0)).unwrap(),
                data_ratings.get(1).unwrap().parse::<i64>().or_else(|_| Ok::<i64, i64>(0)).unwrap(),
                data_ratings.get(2).unwrap().parse::<i64>().or_else(|_| Ok::<i64, i64>(0)).unwrap()
            ));
        }
    }
}

type JSON = serde_json::Value;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/character.graphql",
    response_derives = "Debug",
)]
pub struct CharacterView;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/schema.graphql",
    query_path = "graphql/rate_limit.graphql",
    response_derives = "Debug",
)]
pub struct RateLimitView;

pub struct Updater {
    active: bool,
    config: UpdaterConfig,
    gui_data_arc: Option<Arc<Mutex<UpdaterGuiData>>>,
    base_data: UpdaterBaseData,
    players: HashMap<String, HashMap<String, UpdaterPlayer>>,
    update_addon: SystemTime,
    update_queue: Vec<UpdaterPlayer>,
    update_queue_pos: usize,
    wcl_token: String,
    wcl_points_used: f64,
    wcl_points_limit: f64,
    wcl_reset_at: SystemTime
}

impl Updater {
    pub fn new() -> Updater {
        Updater{
            active: true,
            config: Default::default(),
            gui_data_arc: None,
            base_data: Default::default(),
            players: HashMap::new(),
            update_addon: SystemTime::UNIX_EPOCH,
            update_queue: Vec::new(),
            update_queue_pos: 0,
            wcl_token: Default::default(),
            wcl_points_used: Default::default(),
            wcl_points_limit: Default::default(),
            wcl_reset_at: SystemTime::now()
        }
    }

    pub fn stop(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_update_possible(&self) -> bool {
        (self.update_queue_pos < self.update_queue.len()) && !self.config.api_id.is_empty() && !self.config.api_secret.is_empty()
    }

    pub fn get_player(&mut self, realm: &String, player_name: &String) -> &mut UpdaterPlayer {
        let realm_players = self.players.entry(realm.clone()).or_insert_with(|| HashMap::new());
        realm_players.entry(player_name.clone()).or_insert_with(|| {
            UpdaterPlayer{
                realm: realm.as_str().into(), name: player_name.as_str().into(),
                faction: "Unknown".into(), class: 0, level: 0,
                ranking: Default::default(),
                last_update: 0, last_update_logs: 0, last_update_addon: 0,
                update_priority: 0
            }
        })
    }

    fn modify_gui_data(&self, force: bool, callback: impl FnOnce(&mut UpdaterGuiData)) {
        if let Some(gui_data_arc) = &self.gui_data_arc {
            if force {
                callback(&mut gui_data_arc.lock().unwrap());
            } else {
                if let Ok(gui_data_locked) = &mut gui_data_arc.try_lock() {
                    callback(gui_data_locked);
                }
            }
        }
    }

    pub fn set_gui_data(&mut self, gui_data_arc: Arc<Mutex<UpdaterGuiData>>) {
        self.gui_data_arc = Some(gui_data_arc);
    }

    pub fn set_game_dir(&mut self, game_dir: &str) {
        self.config.game_dir = game_dir.into();
        self.write_config();
        self.read_addon_data();
    }

    pub fn set_api_id(&mut self, api_id: &str) {
        self.config.api_id = api_id.into();
        self.write_config();
    }

    pub fn set_api_secret(&mut self, api_secret: &str) {
        self.config.api_secret = api_secret.into();
        self.write_config();
    }

    pub fn read_addon_data(&mut self) {
        let mut realm_first: Option<String> = None;
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
                    let addon_lua_meta = fs::metadata(addon_lua_saved.clone()).unwrap();
                    let addon_lua_mod = addon_lua_meta.modified().unwrap();
                    if addon_lua_mod > self.update_addon {
                        self.update_addon = addon_lua_mod;
                    }
                    let lua = Lua::new();
                    let data_raw = fs::read_to_string(addon_lua_saved).unwrap();
                    if !lua.load(data_raw.as_str()).exec().is_err() {
                        let data: Table = lua.globals().get("LogTrackerDB").unwrap();
                        let data_realms: Table = data.get("playerData").unwrap();
                        for pair_realm in data_realms.pairs::<String, Table>() {
                            let (realm_name, player_list) = pair_realm.unwrap();
                            if realm_first.is_none() {
                                realm_first = Some(realm_name.clone());
                            }
                            for pair_player in player_list.pairs::<String, Table>() {
                                let (player_name, player_details) = pair_player.unwrap();
                                let player_updated: i64 = player_details.get("lastUpdate").unwrap();
                                let player_updated_logs: i64 = player_details.get("lastUpdateLogs").or_else(|_| Ok::<i64, i64>(0)).unwrap();
                                let mut player = &mut self.get_player(&realm_name, &player_name);
                                player.faction = player_details.get("faction").or_else(|_| Ok::<String, String>("Unknown".to_string())).unwrap().as_str().into();
                                player.class = player_details.get("class").unwrap();
                                player.level = player_details.get("level").unwrap();
                                player.last_update = player_updated;
                                player.last_update_logs = player_updated_logs;
                                player.last_update_addon = player_updated;
                            }
                        }
                    }
                }
            }
        }
        let mut addon_lua_base_data = PathBuf::from(game_dir.clone());
        addon_lua_base_data.push("Interface");
        addon_lua_base_data.push("AddOns");
        addon_lua_base_data.push("LogTracker_BaseData");
        addon_lua_base_data.push("LogTracker_BaseData.lua");
        if addon_lua_base_data.is_file() {
            let lua = Lua::new();
            let data_raw = fs::read_to_string(addon_lua_base_data).unwrap();
            if !lua.load(data_raw.as_str()).exec().is_err() {
                let data: Table = lua.globals().get("LogTracker_BaseData").unwrap();
                let data_classes: Table = data.get("classes").unwrap();
                for pair_class in data_classes.pairs::<String, Table>() {
                    let (class_ident, class_details) = pair_class.unwrap();
                    let base_data_class = self.base_data.classes.entry(class_ident).or_default();
                    base_data_class.id = class_details.get("id").unwrap();
                    base_data_class.name = class_details.get("name").unwrap();
                    base_data_class.slug = class_details.get("slug").unwrap();
                    let data_specs: Table = class_details.get("specs").unwrap();
                    for pair_spec in data_specs.pairs::<String, Table>() {
                        let (spec_ident, spec_details) = pair_spec.unwrap();
                        let base_data_spec = base_data_class.specs.entry(spec_ident).or_default();
                        base_data_spec.id = spec_details.get("id").unwrap();
                        base_data_spec.name = spec_details.get("name").unwrap();
                        base_data_spec.slug = spec_details.get("slug").unwrap();
                        base_data_spec.metric = spec_details.get("metric").unwrap();
                    }
                }
                let data_region_by_server_slug: Table = data.get("regionByServerSlug").unwrap();
                for pair_region_by_server_slug in data_region_by_server_slug.pairs::<String, String>() {
                    let (server_slug, server_region) = pair_region_by_server_slug.unwrap();
                    self.base_data.region_by_server_slug.insert(server_slug, server_region);
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
                let data: Table = lua.globals().get("LogTracker_AppData").unwrap();
                for pair_realm in data.pairs::<String, Table>() {
                    let (realm_name, player_list) = pair_realm.unwrap();
                    if realm_first.is_none() {
                        realm_first = Some(realm_name.clone());
                    }
                    for pair_player in player_list.pairs::<String, Table>() {
                        let (player_name, player_details) = pair_player.unwrap();
                        let import_last_update: i64 = player_details.get(4).unwrap();
                        let player = &mut self.get_player(&realm_name, &player_name);
                        if import_last_update > player.last_update {
                            player.level = player_details.get(1).unwrap();
                            player.faction = player_details.get(2).unwrap();
                            player.class = player_details.get(3).unwrap();
                            player.last_update = import_last_update;
                            player.last_update_logs = import_last_update;
                            let player_encounters: Table = player_details.get(5).unwrap();
                            for pair_encounter in player_encounters.pairs::<String, Table>() {
                                let (encounter_ident, encounter_details) = pair_encounter.unwrap();
                                let player_ranking = player.ranking.entry(encounter_ident).or_default();
                                player_ranking.update_from_lua(encounter_details);
                            }
                        }
                    }
                }
                // TODO: Load previously exported entries
            }
        }
        self.modify_gui_data(true, |gui_data| {
            if let Some(realm_name) = realm_first {
                let gui_manual_realm = &mut gui_data.manual_realm;
                if gui_manual_realm.is_empty() {
                    *gui_manual_realm = realm_name.clone();
                }
            }
        });
        self.rewrite_update_queue();
    }

    pub fn write_addon_data(&mut self) {
        // Serialize data for export
        let mut result = "LogTracker_AppData = {\n".to_string();
        let mut realms: Vec<String> = Vec::new();
        for (realm, player_list) in self.players.iter() {
            let mut realm_str = String::from("  [\"");
            realm_str.push_str(realm);
            realm_str.push_str("\"] = {\n");
            let mut players: Vec<String> = Vec::new();
            for (name, player) in player_list.iter() {
                if player.last_update_addon < player.last_update {
                    let mut data_player: Vec<String> = Vec::new();
                    data_player.push(player.level.to_string());
                    data_player.push(format!("\"{}\"", player.faction));
                    data_player.push(player.class.to_string());
                    data_player.push(player.last_update.to_string());
                    let mut zones: Vec<String> = Vec::new();
                    for (zone_ident, ranking) in player.ranking.iter() {
                        let mut allstars: Vec<String> = Vec::new();
                        for (spec_id, best, median) in ranking.allstar_ratings.iter() {
                            let allstar_str = format!("{{{},{},{}}}", spec_id, best, median);
                            allstars.push(allstar_str);
                        }
                        let mut encounters: Vec<String> = Vec::new();
                        for (spec_id, best, median) in ranking.encounter_ratings.iter() {
                            let encounter_str = format!("{},{},{}", spec_id, best, median);
                            encounters.push(encounter_str);
                        }
                        let zone_str = format!("[\"{}\"] = {{{},{},{{{}}},\"{}\"}}",
                            zone_ident, ranking.encounters, ranking.encounters_killed, allstars.join(","), encounters.join("|")
                        );
                        zones.push(zone_str);
                    }
                    data_player.push(format!("{{ {} }}", zones.join(",")));
                    let mut player_str = String::from("    [\"");
                    player_str.push_str(name);
                    player_str.push_str("\"] = {");
                    player_str.push_str(&data_player.join(","));
                    player_str.push_str("}");
                    players.push(player_str);
                }
            }
            realm_str.push_str(&players.join(",\n"));
            realm_str.push_str("\n  }");
            realms.push(realm_str);
        }
        result.push_str(&realms.join(",\n"));
        result.push_str("\n}");
        // Write to disk
        let game_dir_str = String::from(self.config.game_dir.clone());
        let game_dir = PathBuf::from(game_dir_str);
        let mut addon_lua_import = PathBuf::from(game_dir);
        addon_lua_import.push("Interface");
        addon_lua_import.push("AddOns");
        addon_lua_import.push("LogTracker");
        addon_lua_import.push("AppData.lua");
        let mut file = File::create(addon_lua_import).unwrap();
        file.write_all(result.as_bytes())
            .expect("Failed to write player data");
    }

    pub fn rewrite_update_queue(&mut self) {
        let now = i64::try_from(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()).unwrap();
        self.update_queue_pos = 0;
        self.update_queue.clear();
        for pair_realm in self.players.iter() {
            let (_realm_name, player_list) = pair_realm;
            for pair_player in player_list.iter() {
                let (_player_name, player_details) = pair_player;
                let last_seen = now - player_details.last_update;
                let last_updated = now - player_details.last_update_logs;
                if player_details.last_update_logs == 0 {
                    let mut queue_player = player_details.clone();
                    queue_player.update_priority = 3;
                    self.update_queue.push(queue_player);
                } else if (last_seen < UPDATE_INTERVAL_FAST) && (last_updated > UPDATE_INTERVAL_FAST) {
                    let mut queue_player = player_details.clone();
                    queue_player.update_priority = 2;
                    self.update_queue.push(queue_player);
                } else if last_updated > UPDATE_INTERVAL_SLOW {
                    let mut queue_player = player_details.clone();
                    queue_player.update_priority = 1;
                    self.update_queue.push(queue_player);
                }
            }
        }
        self.update_queue.sort_by(|a, b| {
            if a.update_priority != b.update_priority {
                b.update_priority.cmp(&a.update_priority)
            } else if a.last_update_logs != b.last_update_logs {
                a.last_update_logs.cmp(&b.last_update_logs)
            } else {
                b.last_update.cmp(&a.last_update)
            }
        });
    }

    pub fn load_config(&mut self) {
        let mut config_path = home::home_dir().unwrap();
        config_path.push(".logtrackerapp");
        let config_meta = fs::metadata(config_path.to_owned());
        if config_meta.is_ok() && config_meta.unwrap().is_file() {
            let data = fs::read_to_string(config_path).unwrap();
            self.config = serde_json::from_str(data.as_str()).unwrap();
            if let Some(gui_data_arc) = &self.gui_data_arc {
                let gui_data = &mut gui_data_arc.lock().unwrap();
                gui_data.game_dir = self.config.game_dir.to_string();
                gui_data.api_id = self.config.api_id.to_string();
                gui_data.api_secret = self.config.api_secret.to_string();
            }
        }
    }

    pub fn write_config(&self) {
        let mut config_path = home::home_dir().unwrap();
        config_path.push(".logtrackerapp");
        let mut file = File::create(config_path).unwrap();
        let data = serde_json::to_string(&self.config);
        file.write_all(data.unwrap().as_bytes())
            .expect("Failed to write configuration");
    }

    pub fn update_gui(&self) {
        self.modify_gui_data(false, |gui_data| {
            if let Some(ctx) = &gui_data.ctx {
                ctx.request_repaint();
            }
        });
    }

    pub fn update_addon(&mut self) {
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
                    let addon_lua_meta = fs::metadata(addon_lua_saved.clone()).unwrap();
                    let addon_lua_mod = addon_lua_meta.modified().unwrap();
                    if addon_lua_mod > self.update_addon {
                        // Reload current addon data and rebuild queue
                        info!("Addon file {} changed! Updating...", addon_lua_saved.to_str().unwrap());
                        self.write_addon_data();
                        self.read_addon_data();
                    }
                }
            }
        }
    }

    pub fn update_next(&mut self) -> bool {
        if !self.is_update_possible() {
            sleep(Duration::new(1, 0));
            return false;
        }
        let update_index = self.update_queue_pos;
        let update_count = self.update_queue.len();
        if self.wcl_points_limit > 0.0 {
            let wcl_points_left = self.wcl_points_limit - self.wcl_points_used;
            let wcl_reserve_time = self.wcl_reset_at - Duration::new(300, 0);
            if (wcl_points_left < 600.0) && (SystemTime::now() < wcl_reserve_time) {
                self.modify_gui_data(false, |gui_data| {
                    let points_reserve_dt: DateTime<Local> = wcl_reserve_time.into();
                    let points_reset_dt: DateTime<Local> = self.wcl_reset_at.into();
                    let status_text = format!("Updated {} / {} - Reserving {} points until {} (Reset at {})",
                        self.update_queue_pos, update_count, wcl_points_left.round(), points_reserve_dt.format("%R"), points_reset_dt.format("%R")
                    );
                    info!("Status: {}", status_text);
                    gui_data.status_text = status_text;
                });
                return false;
            }
        }
        self.update_queue_pos += 1;
        let player = self.update_queue.get(update_index).unwrap();
        if self.update_player(player.clone()) {
            self.modify_gui_data(false, |gui_data| {
                let status_text = format!("Updated {} / {} ({} / {} points used)",
                    self.update_queue_pos, update_count, self.wcl_points_used.round(), self.wcl_points_limit.round()
                );
                info!("Status: {}", status_text);
                gui_data.status_text = status_text;
            });
            self.update_gui();
            return true;
        } else {
            self.modify_gui_data(false, |gui_data| {
                let points_reset_dt: DateTime<Local> = self.wcl_reset_at.into();
                let status_text = match self.wcl_points_limit {
                    x if x == 0.0 => format!("Rate limit reached! Reset time is unknown"),
                    _ => format!("Rate limit reached! Reset at {}", points_reset_dt.format("%R"))
                };
                info!("Status: {}", status_text);
                gui_data.status_text = status_text;
            });
            self.update_gui();
            return false;
        }
    }

    pub fn update_player(&mut self, mut player: UpdaterPlayer) -> bool {
        self.auth();
        let region = self.base_data.region_by_server_slug.get(&player.realm.to_lowercase().to_string());
        if region.is_none() {
            return false;
        }
        let zone_id = 1017;
        let (character, character_query) = self.query_character(
            player.name.to_string(), player.realm.to_string(), region.unwrap().to_string(), zone_id, player.class
        );
        if let Some(data) = character {
            if let Some(data_char) = data.character_data.unwrap().character {
                if data_char.class_id > 0 {
                    player.class = data_char.class_id;
                    let mut spec_failed = false;
                    let base_data_class = self.base_data.classes.get(&player.class.to_string()).unwrap();
                    for zone_size in vec![10, 25] {
                        let ranking_id = format!("{}-{}", zone_id, zone_size);
                        let ranking = player.ranking.entry(ranking_id).or_default();
                        ranking.clear();
                        for spec_index in 1..=5 {
                            if let Some(spec_details) = base_data_class.specs.get(&spec_index.to_string()) {
                                let data_json_opt = match (zone_size as i64, spec_index) {
                                    (25, 1) => data_char.zone_rankings25_spec1.as_ref(),
                                    (25, 2) => data_char.zone_rankings25_spec2.as_ref(),
                                    (25, 3) => data_char.zone_rankings25_spec3.as_ref(),
                                    (25, 4) => data_char.zone_rankings25_spec4.as_ref(),
                                    (25, 5) => data_char.zone_rankings25_spec5.as_ref(),
                                    (10, 1) => data_char.zone_rankings10_spec1.as_ref(),
                                    (10, 2) => data_char.zone_rankings10_spec2.as_ref(),
                                    (10, 3) => data_char.zone_rankings10_spec3.as_ref(),
                                    (10, 4) => data_char.zone_rankings10_spec4.as_ref(),
                                    (10, 5) => data_char.zone_rankings10_spec5.as_ref(),
                                    _ => None
                                };
                                if let Some(data_json) = data_json_opt {
                                    // Debug
                                    /*
                                    let mut config_path = home::home_dir().unwrap();
                                    config_path.push("LogTrackerDebug");
                                    config_path.push(format!("{}-{}-spec{}.json", player.name, player.realm, spec_index));
                                    let mut file = File::create(config_path).unwrap();
                                    file.write_all(serde_json::to_string_pretty(data_json).unwrap().as_bytes()).unwrap();
                                    */
                                    // ----------------
                                    ranking.update_from_json(data_json, spec_details.id);
                                } else {
                                    spec_failed = true;
                                }
                            }
                        }                        
                    }
                    // Output debug if some spec failed
                    if spec_failed {
                        if let Some(character_json) = &character_query {
                            info!("No result for query: {}", character_json);
                        }
                    }
                }
            }
            // Debug
            /*
            let mut config_path = home::home_dir().unwrap();
            config_path.push("LogTrackerDebug");
            config_path.push(format!("{}-{}-query.json", player.name, player.realm));
            let mut file = File::create(config_path).unwrap();
            file.write_all(character_query.unwrap().as_bytes()).unwrap();
            */
            // ----------------
            player.last_update = i64::try_from(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()).unwrap();
            player.last_update_logs = player.last_update;
            // Write into player list
            let realm_players = self.players.entry(player.realm.to_string()).or_insert_with(|| HashMap::new());
            realm_players.insert(player.name.to_string(), player);
        } else {
            if !self.update_api_limit() {
                return false;
            }
            // Request successful, but no logs available
            player.last_update = i64::try_from(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()).unwrap();
            player.last_update_logs = player.last_update;
            // Write into player list
            let realm_players = self.players.entry(player.realm.to_string()).or_insert_with(|| HashMap::new());
            realm_players.insert(player.name.to_string(), player);
            return true;
        }
        true
    }

    pub fn update_api_limit(&mut self) -> bool {
        self.auth();
        let rate_limit = self.query_rate_limit();
        if let Some(rate_limit_response) = rate_limit {
            let rate_limit_data = rate_limit_response.rate_limit_data.unwrap();
            info!(
                "Rate limit info: {} / {} points spent, reset in {} seconds", 
                rate_limit_data.points_spent_this_hour, rate_limit_data.limit_per_hour, rate_limit_data.points_reset_in
            );
            self.wcl_points_limit = rate_limit_data.limit_per_hour as f64;
            self.wcl_points_used = rate_limit_data.points_spent_this_hour;
            self.wcl_reset_at = SystemTime::now() + Duration::new(u64::try_from(rate_limit_data.points_reset_in).unwrap_or_default() + 60, 0);
            true
        } else {
            false
        }
    }

    fn auth(&mut self) -> String {
        if !self.wcl_token.is_empty() {
            return self.wcl_token.clone();
        }
        let client = BasicClient::new(
            ClientId::new(self.config.api_id.to_string()),
            Some(ClientSecret::new(self.config.api_secret.to_string())),
            AuthUrl::new("https://www.warcraftlogs.com/oauth/authorize".to_string()).unwrap(),
            Some(TokenUrl::new("https://www.warcraftlogs.com/oauth/token".to_string()).unwrap()),
        );
        
        let token_result: StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType> = client
            .exchange_client_credentials()
            .request(http_client).unwrap();
        let mut auth_string: String = "Bearer ".to_string();
        auth_string.push_str(token_result.access_token().secret().clone().as_str());
        self.wcl_token = auth_string;
        self.wcl_token.clone()
    }

    pub fn query_character_metric(&self, spec: &UpdaterBaseDataClassSpec) -> Option<character_view::CharacterRankingMetricType> {
        let metric_str: &str = &spec.metric.clone();
        match metric_str {
            "hps" => Some(character_view::CharacterRankingMetricType::hps),
            _ => Some(character_view::CharacterRankingMetricType::dps)
        }
    }

    pub fn query_character(&self, name: String, server_slug: String, server_region: String, zone_id: i64, class_id: i64) 
        -> (Option<character_view::ResponseData>, Option<String>)
    {
        let mut vars = character_view::Variables {
            name: name, server_slug: server_slug, server_region: server_region, 
            zone_id: zone_id,
            query_spec1: false, query_spec1_name: None, query_spec1_metric: None,
            query_spec2: false, query_spec2_name: None, query_spec2_metric: None,
            query_spec3: false, query_spec3_name: None, query_spec3_metric: None,
            query_spec4: false, query_spec4_name: None, query_spec4_metric: None,
            query_spec5: false, query_spec5_name: None, query_spec5_metric: None
        };
        if let Some(base_data_class) = self.base_data.classes.get(&class_id.to_string()) {
            for spec_index in 1..=5 {
                if let Some(spec_details) = base_data_class.specs.get(&spec_index.to_string()) {
                    match spec_index {
                        1 => {
                            vars.query_spec1 = true;
                            vars.query_spec1_name = Some(spec_details.slug.to_string());
                            vars.query_spec1_metric = self.query_character_metric(spec_details);
                        },
                        2 => {
                            vars.query_spec2 = true;
                            vars.query_spec2_name = Some(spec_details.slug.to_string());
                            vars.query_spec2_metric = self.query_character_metric(spec_details);
                        },
                        3 => {
                            vars.query_spec3 = true;
                            vars.query_spec3_name = Some(spec_details.slug.to_string());
                            vars.query_spec3_metric = self.query_character_metric(spec_details);
                        },
                        4 => {
                            vars.query_spec4 = true;
                            vars.query_spec4_name = Some(spec_details.slug.to_string());
                            vars.query_spec4_metric = self.query_character_metric(spec_details);
                        },
                        5 => {
                            vars.query_spec5 = true;
                            vars.query_spec5_name = Some(spec_details.slug.to_string());
                            vars.query_spec5_metric = self.query_character_metric(spec_details);
                        }
                        _ => {}
                    }
                }
            }
            let client = Client::builder()
                .user_agent("graphql-rust/0.10.0")
                .default_headers(
                    std::iter::once((
                        reqwest::header::AUTHORIZATION,
                        reqwest::header::HeaderValue::from_str(self.wcl_token.as_str().clone()).unwrap()
                    ))
                    .collect()
                )
                .build().unwrap();
            let vars_string = serde_json::to_string_pretty(&vars).unwrap();
            let response_body = post_graphql::<CharacterView, _>(&client, "https://classic.warcraftlogs.com/api/v2/client", vars);
            if let Err(e) = response_body {
                warn!("Application error: {e}");
                return (None, Some(vars_string));
            }
            (response_body.unwrap().data, Some(vars_string))
        } else {
            (None, None)
        }
    }

    pub fn query_rate_limit(&self) -> Option<rate_limit_view::ResponseData> {
        let vars = rate_limit_view::Variables {};
        let client = Client::builder()
            .user_agent("graphql-rust/0.10.0")
            .default_headers(
                std::iter::once((
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(self.wcl_token.as_str().clone()).unwrap()
                ))
                .collect()
            )
            .build().unwrap();
        let response_body = post_graphql::<RateLimitView, _>(&client, "https://classic.warcraftlogs.com/api/v2/client", vars);
        if let Err(e) = response_body {
            warn!("Application error: {e}");
            return None;
        }
        response_body.unwrap().data
    }

}