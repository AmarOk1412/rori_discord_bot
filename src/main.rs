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

extern crate env_logger;
#[macro_use]
extern crate log;
extern crate serenity;
extern crate serde;
extern crate serde_json;

pub mod discord;

use discord::Bot;
use serde_json::{Value, from_str};
use std::io::prelude::*;
use std::fs::File;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

fn main() {
    // 0. Init logging
    env_logger::init();

    // 1. Read current config
    let mut file = File::open("config.json").ok()
            .expect("Config file not found");
    let mut config = String::new();
    file.read_to_string(&mut config).ok()
        .expect("failed to read!");
    let config: Value = from_str(&*config).ok()
                        .expect("Incorrect config file. Please check config.json");

    // 2. Run discord bot
    let stop = Arc::new(AtomicBool::new(false));
    let _stop_cloned = stop.clone();
    let handle_discord_event = thread::spawn(move || {
        Bot::run(&config["discord_secret_token"].as_str().unwrap_or(""));
    });
    // stop.store(false, Ordering::SeqCst);
    let _ = handle_discord_event.join();
}
