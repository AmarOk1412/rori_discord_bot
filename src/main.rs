/**
 * Copyright (c) 2018, Sébastien Blin <sebastien.blin@enconn.fr>
 * All rights reserved.
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * * Redistributions of source code must retain the above copyright
 *  notice, this list of conditions and the following disclaimer.
 * * Redistributions in binary form must reproduce the above copyright
 *  notice, this list of conditions and the following disclaimer in the
 *  documentation and/or other materials provided with the distribution.
 * * Neither the name of the University of California, Berkeley nor the
 *  names of its contributors may be used to endorse or promote products
 *  derived from this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND ANY
 * EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
 * WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * DISCLAIMED. IN NO EVENT SHALL THE REGENTS AND CONTRIBUTORS BE LIABLE FOR ANY
 * DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
 * (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 * LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
 * ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
 * (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
 * SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 **/

extern crate dbus;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate reqwest;
extern crate serenity;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate time;


pub mod discord;
pub mod rori;

use discord::Bot;
use discord::DiscordMsg;
use rori::endpoint::Endpoint;
use serde_json::{Value, from_str};
use std::io::prelude::*;
use std::io::{stdin,stdout,Write};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;

/**
 * Generate a config file
 */

#[derive(Serialize, Deserialize)]
pub struct ConfigFile {
    discord_secret_token: String,
    ring_id: String,
    rori_server: String,
    rori_ring_id: String
}

fn clean_string(string: String) -> String {
    let mut s = string.clone();
    if let Some('\n') = s.chars().next_back() {
        s.pop();
    }
    if let Some('\r') = s.chars().next_back() {
        s.pop();
    }
    s
}

fn create_config_file() {
    println!("Config file not found. Please answer to following questions:");
    let mut s = String::new();
    print!("Discord secret token: ");
    let _ = stdout().flush();
    stdin().read_line(&mut s).expect("Did not enter a correct string");
    s = clean_string(s);
    let discord_secret_token = s.clone();

    let mut s = String::new();
    print!("RORI server: ");
    let _ = stdout().flush();
    stdin().read_line(&mut s).expect("Did not enter a correct string");
    s = clean_string(s);
    let rori_server = s.clone();
    let rori_ring_id = Endpoint::get_ring_id(&rori_server, &String::from("rori"));
    if rori_ring_id == "" {
        error!("Cannot connect to this RORI. Abort");
        return;
    }

    println!("Create an account? y/N: ");
    let _ = stdout().flush();
    let mut s = String::new();
    stdin().read_line(&mut s).expect("Did not enter a correct string");
    s = clean_string(s);
    if s.len() == 0 {
        s = String::from("N");
    }
    if s.to_lowercase() == "y" {
        let mut s = String::new();
        println!("Import archive? y/N: ");
        let _ = stdout().flush();
        stdin().read_line(&mut s).expect("Did not enter a correct string");
        s = clean_string(s);
        if s.len() == 0 {
            s = String::from("N");
        }
        let from_archive = s.to_lowercase() == "y";
        if from_archive {
            println!("path: ");
        } else {
            println!("alias: ");
        }
        let _ = stdout().flush();
        let mut s = String::new();
        stdin().read_line(&mut s).expect("Did not enter a correct string");
        s = clean_string(s);
        let main_info = s.clone();
        println!("password (optional):");
        let _ = stdout().flush();
        let mut s = String::new();
        stdin().read_line(&mut s).expect("Did not enter a correct string");
        s = clean_string(s);
        let password = s.clone();
        Endpoint::add_account(&*main_info, &*password, from_archive);
        // Let some time for the daemon
        let three_secs = Duration::from_millis(3000);
        thread::sleep(three_secs);
    }

    let accounts = Endpoint::get_account_list();
    let mut idx = 0;
    println!("Choose an account:");
    for account in &accounts {
        println!("{}. {}", idx, account);
        idx += 1;
    }
    println!("Your choice:");
    let _ = stdout().flush();
    let mut s = String::new();
    stdin().read_line(&mut s).expect("Did not enter a correct string");
    s = clean_string(s);
    if s.len() == 0 {
        s = String::from("0");
    }
    let s = s.parse::<usize>().unwrap_or(0);
    if s >= accounts.len() {
        return;
    }
    let account = &accounts.get(s).unwrap().id;
    let config = ConfigFile {
        discord_secret_token,
        ring_id: account.clone(),
        rori_server,
        rori_ring_id
    };
    let config = serde_json::to_string_pretty(&config).unwrap_or(String::new());
    let mut file = File::create("config.json").ok().expect("config.json found.");
    let _ = file.write_all(config.as_bytes());

}

fn main() {
    // 0. Init logging
    env_logger::init();
    let mut bridgify = true;

    // 1. Read current config
    // but if no config, create it
    if !Path::new("config.json").exists() {
        bridgify = true;
        create_config_file();
    }
    let mut file = File::open("config.json").ok()
            .expect("Config file not found");
    let mut config = String::new();
    file.read_to_string(&mut config).ok()
        .expect("failed to read!");
    let config: Value = from_str(&*config).ok()
                        .expect("Incorrect config file. Please check config.json");
    let config_cloned = config.clone();

    // 2. Init Ring account
    let stop = Arc::new(AtomicBool::new(false));
    let stop_cloned = stop.clone();
    let user_text = Arc::new(Mutex::new(DiscordMsg::new()));
    let rori_text = Arc::new(Mutex::new(DiscordMsg::new()));
    let user_text_cloned = user_text.clone();
    let rori_text_cloned = rori_text.clone();

    let _handle_signals = thread::spawn(move || {
        let shared_endpoint : Arc<Mutex<Endpoint>> = Arc::new(Mutex::new(
            Endpoint::init(config["ring_id"].as_str().unwrap_or(""),
                           config["rori_ring_id"].as_str().unwrap_or(""))
            .ok().expect("Can't initialize ConfigurationEndpoint"))
        );
        if bridgify {
            Endpoint::bridgify(&shared_endpoint);
        }
        Endpoint::handle_signals(shared_endpoint, stop_cloned, user_text, rori_text);
    });

    // 3. Run discord bot
    let handle_discord_event = thread::spawn(move || {
        Bot::new().run(&config_cloned["discord_secret_token"].as_str().unwrap_or(""),
            user_text_cloned, rori_text_cloned);
    });
    // stop.store(false, Ordering::SeqCst);
    let _ = handle_discord_event.join();
}
