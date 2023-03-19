extern crate serde;
extern crate serde_json;

use std::env::var;
use std::path::PathBuf;
use std::fs::{self, File};
use std::io::Write;
use std::time::{Duration, SystemTime};
use std::thread::sleep;
use std::collections::HashMap;
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
    api_key: Box<str>,
    api_secret: Box<str>
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
    last_update_addon: i64
}


#[derive(Clone, Default)]
pub struct UpdaterBaseData {
    classes: HashMap<String, UpdaterBaseDataClass>
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
}

#[derive(Clone, Default)]
pub struct UpdaterRanking {
    encounters: i64,
    encounters_killed: i64,
    allstar_ratings: Vec<(i64,i64)>,
    encounter_ratings: Vec<(i64,i64,i64)>
}

impl UpdaterRanking {
    pub fn update_from_json(&mut self, data: serde_json::Value, class_id: i64, base_data: &UpdaterBaseData) {
        // Clear values
        self.encounters = 0;
        self.encounters_killed = 0;
        self.allstar_ratings.clear();
        self.encounter_ratings.clear();
        // Fill with new data
        let base_data_class = base_data.classes.get(&class_id.to_string()).unwrap();
        let allstars = data.get("allStars").unwrap().as_array().unwrap();
        for allstars_spec in allstars.iter() {
            let spec_ident = allstars_spec.get("spec").unwrap().as_str().unwrap();
            let spec_rating = allstars_spec.get("rankPercent").unwrap().as_f64().unwrap();
            let base_data_spec = base_data_class.specs.get(&spec_ident.to_string()).unwrap();
            self.allstar_ratings.push((base_data_spec.id, spec_rating.round() as i64));
        }
        let encounters = data.get("rankings").unwrap().as_array().unwrap();
        for encounter_rank in encounters.iter() {
            self.encounters += 1;
            let spec_raw = encounter_rank.get("spec").unwrap();
            if !spec_raw.is_null() {
                self.encounters_killed += 1;
                let spec_ident = encounter_rank.get("spec").unwrap().as_str().unwrap();
                let spec_rating = encounter_rank.get("rankPercent").unwrap().as_f64().unwrap();
                let spec_median = encounter_rank.get("medianPercent").unwrap().as_f64().unwrap();
                let base_data_spec = base_data_class.specs.get(&spec_ident.to_string()).unwrap();
                self.encounter_ratings.push((base_data_spec.id, spec_rating.round() as i64, spec_median.round() as i64));
            } else {
                self.encounter_ratings.push((0,0,0));
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
            self.allstar_ratings.push((allstar_details.get(1).unwrap(), allstar_details.get(2).unwrap()));
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
    base_data: UpdaterBaseData,
    players: HashMap<String, HashMap<String, UpdaterPlayer>>,
    update_addon: SystemTime,
    update_queue: Vec<UpdaterPlayer>,
    update_queue_pos: usize,
    wcl_token: String
}

impl Updater {
    pub fn new() -> Updater {
        Updater{
            active: true,
            config: Default::default(),
            base_data: Default::default(),
            players: HashMap::new(),
            update_addon: SystemTime::UNIX_EPOCH,
            update_queue: Vec::new(),
            update_queue_pos: 0,
            wcl_token: Default::default()
        }
    }

    pub fn stop(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
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
                faction: "Unknown".into(), class: 0, level: 0,
                ranking: Default::default(),
                last_update: 0, last_update_logs: 0, last_update_addon: 0
            }
        })
    }

    pub fn set_game_dir(&mut self, game_dir: &str) {
        self.config.game_dir = game_dir.into();
        self.write_config();
        self.read_addon_data();
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
                let data: Table = lua.globals().get("LogTracker_AppData").unwrap();
                for pair_realm in data.pairs::<String, Table>() {
                    let (realm_name, player_list) = pair_realm.unwrap();
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
                        for (spec_id, score) in ranking.allstar_ratings.iter() {
                            let mut allstar_str = String::from("{");
                            allstar_str.push_str(&spec_id.to_string());
                            allstar_str.push_str(",");
                            allstar_str.push_str(&score.to_string());
                            allstar_str.push_str("}");
                            allstars.push(allstar_str);
                        }
                        let mut encounters: Vec<String> = Vec::new();
                        for (spec_id, best, median) in ranking.encounter_ratings.iter() {
                            let mut encounter_str = String::from(&spec_id.to_string());
                            encounter_str.push_str(",");
                            encounter_str.push_str(&best.to_string());
                            encounter_str.push_str(",");
                            encounter_str.push_str(&median.to_string());
                            encounters.push(encounter_str);
                        }
                        let mut zone_str = String::from("[\"");
                        zone_str.push_str(zone_ident);
                        zone_str.push_str("\"] = {");
                        zone_str.push_str(&ranking.encounters.to_string());
                        zone_str.push_str(",");
                        zone_str.push_str(&ranking.encounters_killed.to_string());
                        zone_str.push_str(",{");
                        zone_str.push_str(&allstars.join(","));
                        zone_str.push_str("},\"");
                        zone_str.push_str(&encounters.join("|"));
                        zone_str.push_str("\"}");
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
                if (last_seen < UPDATE_INTERVAL_FAST) && (last_updated > UPDATE_INTERVAL_FAST) {
                    self.update_queue.push(player_details.clone());
                } else if last_updated > UPDATE_INTERVAL_SLOW {
                    self.update_queue.push(player_details.clone());
                }
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

    pub fn update_next(&mut self) -> (usize,usize) {
        let update_index = self.update_queue_pos;
        let update_count = self.update_queue.len();
        if (update_index >= update_count) || self.config.api_key.is_empty() || self.config.api_secret.is_empty() {
            sleep(Duration::new(1, 0));
            return (update_index, update_count);
        }
        self.auth();
        self.update_queue_pos += 1;
        let player = self.update_queue.get(update_index).unwrap();
        let zone_id = 1017;
        let character = self.query_character(
            player.name.to_string(), player.realm.to_string(), "EU".to_string(), zone_id, false
        );
        let player = self.update_queue.get_mut(update_index).unwrap();
        if let Some(data) = character {
            if let Some(data_char) = data.character_data.unwrap().character {
                if data_char.class_id > 0 {
                    player.class = data_char.class_id;
                    if let Some(data_json) = data_char.zone_rankings_dmg25 {
                        let mut ranking_id = zone_id.to_string();
                        ranking_id.push_str("-25");
                        let ranking = player.ranking.entry(ranking_id).or_default();
                        ranking.update_from_json(data_json, player.class, &self.base_data);
                    }
                    if let Some(data_json) = data_char.zone_rankings_heal25 {
                        let mut ranking_id = zone_id.to_string();
                        ranking_id.push_str("-25");
                        let ranking = player.ranking.entry(ranking_id).or_default();
                        ranking.update_from_json(data_json, player.class, &self.base_data);
                    }
                    if let Some(data_json) = data_char.zone_rankings_dmg10 {
                        let mut ranking_id = zone_id.to_string();
                        ranking_id.push_str("-10");
                        let ranking = player.ranking.entry(ranking_id).or_default();
                        ranking.update_from_json(data_json, player.class, &self.base_data);                
                    }
                    if let Some(data_json) = data_char.zone_rankings_heal10 {
                        let mut ranking_id = zone_id.to_string();
                        ranking_id.push_str("-10");
                        let ranking = player.ranking.entry(ranking_id).or_default();
                        ranking.update_from_json(data_json, player.class, &self.base_data);
                    }
                    player.last_update = i64::try_from(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()).unwrap();
                    player.last_update_logs = player.last_update;
                    // Write into player list
                    let realm_players = self.players.entry(player.realm.to_string()).or_insert_with(|| HashMap::new());
                    realm_players.insert(player.name.to_string(), player.clone());
                }
            }
        } else {
            let rate_limit = self.query_rate_limit();
            if let Some(rate_limit_response) = rate_limit {
                let rate_limit_data = rate_limit_response.rate_limit_data.unwrap();
                info!(
                    "Rate limit info: {} / {} points spent, reset in {}", 
                    rate_limit_data.limit_per_hour, rate_limit_data.points_spent_this_hour, rate_limit_data.points_reset_in
                );
            } else {
                info!(
                    "Failed to obtain rate limit, assuming limit is reached! Pausing for 5 minutes..."
                );
                sleep(Duration::new(300, 0));
            }
        }
        return (update_index+1, update_count);
    }

    fn auth(&mut self) -> String {
        if !self.wcl_token.is_empty() {
            return self.wcl_token.clone();
        }
        let client = BasicClient::new(
            ClientId::new(self.config.api_key.to_string()),
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

    pub fn query_character(&self, name: String, server_slug: String, server_region: String, zone_id: i64, query_heal: bool) -> Option<character_view::ResponseData> {
        let vars = character_view::Variables {
            name: name, server_slug: server_slug, server_region: server_region, 
            zone_id: zone_id, query_damage: true, query_heal: query_heal
        };
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
        let response_body = post_graphql::<CharacterView, _>(&client, "https://classic.warcraftlogs.com/api/v2/client", vars);
        if let Err(e) = response_body {
            warn!("Application error: {e}");
            return None;
        }
        response_body.unwrap().data
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