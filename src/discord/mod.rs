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

use serenity::CACHE;
use serenity::model::channel::{GuildChannel, Message};
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::utils::MessageBuilder;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;


/**
 * Represent a RING account, just here to store informations.
 **/
#[derive(Debug, Clone)]
pub struct Bot;

struct Handler {
    user_say: Arc<Mutex<String>>,
    channel: Arc<Mutex<Option<GuildChannel>>>,
}

impl EventHandler for Handler {
    fn message(&self, _: Context, msg: Message) {
        if msg.author.id != CACHE.read().user.id {
            // TODO: for now, just forward content
            *self.user_say.lock().unwrap() = msg.content.clone();
        }
    }

    fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
        for guild in ready.guilds {
            let server_name = guild.id().to_partial_guild().unwrap().name;
            for (_, chan) in guild.id().channels().ok().expect("No channels!") {
                // TODO: this is temporary. Just forward to one channel
                if server_name == "RORI" && chan.name() == "general" {
                    *self.channel.lock().unwrap() = Some(chan);
                }
            }
        }
    }
}

impl Bot {
    pub fn run(secret_token: &str, user_say: Arc<Mutex<String>>, rori_say: Arc<Mutex<String>>) {
        // Configure the client with your Discord bot token in the environment.
        let channel = Arc::new(Mutex::new(None));
        let channel_cloned = channel.clone();
        let mut client = Client::new(secret_token, Handler { user_say, channel })
                         .expect("Error initializing RORI client");

        let answer_thread = thread::spawn(move || {
             loop {
                 // TODO: For now, just forward incoming messages to discord
                 let to_say: String = String::from(&*rori_say.lock().unwrap().clone());
                 if !to_say.is_empty() {
                     info!("{}", to_say);
                     let response = MessageBuilder::new()
                         .push(&*to_say)
                         .build();
                     *rori_say.lock().unwrap() = String::new();
                     let channel = &*channel_cloned.lock().unwrap();
                     if let Some(c) = channel {
                         if let Err(why) = c.id.say(&response) {
                             error!("Error sending message: {:?}", why);
                         }
                     }
                 }
                 // Let some time for the daemon
                 let five_hundred_ms = Duration::from_millis(500);
                 thread::sleep(five_hundred_ms);
             }
        });

        if let Err(why) = client.start() {
            error!("Client error: {:?}", why);
        }
        // TODO stop correctly
        let _ = answer_thread.join();
    }
}
