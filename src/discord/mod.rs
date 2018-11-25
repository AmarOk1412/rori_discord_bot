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

use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
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
pub struct Bot {
    ready: Arc<Mutex<Option<Ready>>>,
}

/**
 * Represent a Discord message. Is converted into/from an Interaction
 **/
pub struct DiscordMsg {
    pub id: String,
    pub body: String,
    pub author: String,
    pub channel: String
}

impl DiscordMsg {
    pub fn new() -> Self {
        DiscordMsg {
            id: String::new(),
            body: String::new(),
            author: String::new(),
            channel: String::new(),
        }
    }
}

impl Clone for DiscordMsg {
    fn clone(&self) -> DiscordMsg {
        DiscordMsg {
            id: self.id.clone(),
            body: self.body.clone(),
            author: self.author.clone(),
            channel: self.channel.clone(),
        }
    }
}

/**
 * Shared informations between the Bot and the handler
 */
struct Handler {
    user_say: Arc<Mutex<DiscordMsg>>,
    ready: Arc<Mutex<Option<Ready>>>,
}

impl EventHandler for Handler {
    fn message(&self, _: Context, msg: Message) {
        if msg.content == "/help" {
            let mut usage: String = String::from("Hi! I'm RORI, a free distributed chatterbot.\n");
            usage += "If you want to use this instance as another user.\n";
            usage += "This is some commands:\n";
            usage += "/register <username> for registering a user\n";
            usage += "/unregister for unregistering a user\n";
            usage += "/add_device <device_name> [id] for giving a name to a device\n";
            usage += "/rm_device <device_name> [id] for removing a device\n";
            usage += "/link <id|username> for adding a new device to a user";

            if let Err(why) = msg.channel_id.say(usage) {
                println!("Error sending message: {:?}", why);
            }
        } else {
            // TODO: for now, just forward content
            *self.user_say.lock().unwrap() = DiscordMsg {
                id: msg.id.as_u64().to_string(),
                body: msg.content.clone(),
                author: msg.author.id.as_u64().to_string(),
                channel: msg.channel_id.as_u64().to_string(),
            };
        }
    }

    fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
        *self.ready.lock().unwrap() = Some(ready);
    }
}

impl Bot {
    /**
     * Create a Bot instance
     */
    pub fn new() -> Bot {
        Bot {
            ready: Arc::new(Mutex::new(None))
        }
    }

    /**
     * Main loop of the bot, to do the bridge between RORI and Discord
     * @param self
     * @param secret_token for the bot
     * @param user_say, what the user say for RORI
     * @param rori_say, what RORI say on Discord
     */
    pub fn run(self, secret_token: &str, user_say: Arc<Mutex<DiscordMsg>>, rori_say: Arc<Mutex<DiscordMsg>>) {
        // Configure the client with your Discord bot token in the environment.
        let ready = self.ready.clone();
        let mut client = Client::new(secret_token, Handler { user_say, ready })
                         .expect("Error initializing RORI client");

        let answer_thread = thread::spawn(move || {
            loop {
                // Forward incoming messages to discord
                let to_say: String = String::from(&*rori_say.lock().unwrap().body.clone());
                if !to_say.is_empty() {
                    let channel_id: String = String::from(&*rori_say.lock().unwrap().channel.clone());
                    *rori_say.lock().unwrap() = DiscordMsg::new();
                    info!("{}", to_say);
                    let response = MessageBuilder::new()
                        .push(&*to_say)
                        .build();
                    if let Some(id) = self.get_channel_from_id(&channel_id) {
                        if let Err(why) = id.say(&response) {
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

    /**
     * Retrieve a channel from an id
     * @param self
     * @param id
     * @return the Channel if found, else the default channel if ready or None if not ready.
     */
    fn get_channel_from_id(&self, id: &String) -> Option<ChannelId> {
        let ready = &*self.ready.lock().unwrap();
        if let Some(r) = ready {
            let id = id.parse::<u64>().unwrap_or(0);
            if id != 0 {
                return Some(ChannelId::from(id));
            }
            for guild in &r.guilds {
                let server_name = guild.id().to_partial_guild().unwrap().name;
                for (chan_id, chan) in guild.id().channels().ok().expect("No channels!") {
                    // TODO default channel configuration!
                    if server_name == "RORI" && chan.name() == "general" {
                        return Some(chan_id)
                    }
                }
            }
        }
        None
    }
}
