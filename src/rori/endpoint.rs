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

use dbus::{Connection, ConnectionItem, BusType, Message};
use dbus::arg::{Array, Dict};
use discord::DiscordMsg;
use reqwest;
use rori::account::Account;
use rori::database::Database;
use rori::interaction::Interaction;
use serde_json::{Value, from_str};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use time;

/**
 * This class is used to load RORI accounts and handle signals from Ring.
 * Should be one unique instance of this and is used to access the RORI server
 */
pub struct Endpoint {
    pub account: Account,

    rori_ring_id: String,
    ring_dbus: &'static str,
    configuration_path: &'static str,
    configuration_iface: &'static str,
    current_transactions: HashMap<String, String>
}

impl Endpoint {
    /**
     * Send first /bridgify order
     */
    pub fn bridgify(manager: &Arc<Mutex<Endpoint>>) {
        let datatype = "rori/command";
        let mut payloads: HashMap<&str, &str> = HashMap::new();
        payloads.insert(datatype, "/bridgify");
        let m = manager.lock().unwrap();
        m.send_interaction_to_rori(payloads);
    }
    /**
     * Init the RORI server, the database and retrieve the RING account linked
     * @param ring_id to retrieve
     * @return a Manager if success, else an error
     */
    pub fn init(ring_id: &str, rori_ring_id: &str) -> Result<Endpoint, &'static str> {
        Database::init_db();
        let mut manager = Endpoint {
            account: Account::null(),

            rori_ring_id: String::from(rori_ring_id),
            ring_dbus: "cx.ring.Ring",
            configuration_path: "/cx/ring/Ring/ConfigurationManager",
            configuration_iface: "cx.ring.Ring.ConfigurationManager",
            current_transactions: HashMap::new(),
        };
        manager.account = Endpoint::build_account(ring_id);
        if !manager.account.enabled {
            info!("{} was not enabled. Enable it", ring_id);
            manager.enable_account();
        }
        debug!("Get: {}", manager.account.ring_id);
        if manager.account.ring_id == "" {
            return Err("Cannot build RORI account, please check configuration");
        }
        info!("{}: Account loaded", manager.account.id);
        Ok(manager)
    }

    /**
     * Listen from interresting signals from dbus and call handlers
     * @param self
     */
    pub fn handle_signals(manager: Arc<Mutex<Endpoint>>, stop: Arc<AtomicBool>, user_text: Arc<Mutex<DiscordMsg>>, rori_text: Arc<Mutex<DiscordMsg>>) {
        // Use another dbus connection to listen signals.
        let dbus_listener = Connection::get_private(BusType::Session).unwrap();
        dbus_listener.add_match("interface=cx.ring.Ring.ConfigurationManager,member=incomingAccountMessage").unwrap();
        dbus_listener.add_match("interface=cx.ring.Ring.ConfigurationManager,member=incomingTrustRequest").unwrap();
        dbus_listener.add_match("interface=cx.ring.Ring.ConfigurationManager,member=accountsChanged").unwrap();
        dbus_listener.add_match("interface=cx.ring.Ring.ConfigurationManager,member=registrationStateChanged").unwrap();
        // For each signals, call handlers.
        for i in dbus_listener.iter(100) {

            let mut m = manager.lock().unwrap();
            m.handle_accounts_signals(&i);
            m.handle_registration_changed(&i);
            if let Some((account_id, interaction)) = m.handle_interactions(&i) {
                info!("New interation for {}: {}", account_id, interaction);
                if account_id == m.account.id {
                    if interaction.datatype == "rori/message" {
                        match from_str(&interaction.body) {
                            Ok(j) => {
                                // Only if rori order
                                let j: Value = j;
                                let username = String::from(j["username"].as_str().unwrap_or(""));
                                if j["registered"].to_string() == "true" {
                                    if m.current_transactions.contains_key(&username) {
                                        let _ = Database::add_user(m.current_transactions.get(&username).unwrap(), &username);
                                        m.current_transactions.remove(&username);
                                    } else {
                                        warn!("Registered user found, but no user linked for {}", username);
                                    }
                                } else {
                                    let _ = Database::remove_user(&username);
                                    m.current_transactions.remove(&username);
                                }
                            },
                            _ => {
                                warn!("Message received, but not recognized: {}", interaction.body);
                            }
                        };
                    } else {
                        let mut channel = String::new();
                        if interaction.metadatas.contains_key("ch") {
                            channel = interaction.metadatas["ch"].clone();
                        }
                        *rori_text.lock().unwrap() = DiscordMsg {
                            id: String::new(),
                            body: interaction.body,
                            author: String::new(),
                            channel: channel,
                        };
                    }
                }
            };
            if let Some((account_id, from)) = m.handle_requests(&i) {
                if account_id == m.account.id {
                    info!("New request from {}", from);
                    // TODO
                }
            };
            let mut utext = user_text.lock().unwrap().clone();
            if utext.body != "" {
                *user_text.lock().unwrap() = DiscordMsg::new();
                // Retrieve username of current author
                let username = Database::username(&utext.author);

                let mut datatype = "text/plain";
                if m.is_a_command(&utext.body) {
                    datatype = "rori/command";
                    // If no username
                    if username.len() == 0 {
                        if utext.body.starts_with("/register") {
                            let split: Vec<&str> = utext.body.split(' ').collect();
                            if split.len() < 2 {
                                warn!("register received, but no username detected");
                                continue;
                            }
                            let u = String::from(*split.get(1).unwrap());
                            // If /register, test if no transaction for this username
                            if Database::id(&u).len() > 0 || m.current_transactions.contains_key(&u) {
                                // already registered or in progress, drop request
                                continue;
                            }
                            m.current_transactions.insert(u, utext.author.clone());
                        } else if utext.body.starts_with("/unregister") {
                            // Drop if /unregister
                            continue;
                        }
                    } else {
                        // If username
                        if utext.body.starts_with("/register") {
                            // If /register, drop
                            continue;
                        }
                    }
                }
                let mut payloads: HashMap<&str, &str> = HashMap::new();
                payloads.insert(datatype, &*utext.body);
                payloads.insert("sa", &*username);
                payloads.insert("th", &*utext.id);
                payloads.insert("ch", &*utext.channel);
                m.send_interaction_to_rori(payloads);
            }
            if stop.load(Ordering::SeqCst) {
                break;
            }
        }
    }

    pub fn get_ring_id(nameserver: &String, name: &String) -> String {
        // NOTE/TODO: Remove this line when RORI will generate certificate with Let's Encrypt
        // For now, self signed certificate and local dev, so it's OK
        let client = reqwest::ClientBuilder::new()
                    .danger_accept_invalid_certs(true)
                    .build().unwrap();

        let mut ns = nameserver.clone();
        if ns.find("http") != Some(0) {
            ns = String::from("https://") + &*ns;
        }
        let mut res = match client.get(&*format!("{}/name/{}", ns, name)).send() {
            Ok(res) => res,
            _ => {
                return String::new();
            }
        };

        let mut body: String = String::new();
        let _ = res.read_to_string(&mut body);
        match from_str(&body) {
            Ok(j) => {
                // Only if rori order
                let j: Value = j;
                let addr = j["addr"].to_string();
                if addr.len() > 4 {
                    return String::from(&addr[3..addr.len()-1]);
                }
                return String::new();
            },
            _ => {
                return String::new();
            }
        };
    }

    pub fn get_username_from_api(nameserver: &String, ring_id: &String) -> String {
        // NOTE/TODO: Remove this line when RORI will generate certificate with Let's Encrypt
        // For now, self signed certificate and local dev, so it's OK
        let client = reqwest::ClientBuilder::new()
                    .danger_accept_invalid_certs(true)
                    .build().unwrap();

        let mut ns = nameserver.clone();
        if ns.find("http") != Some(0) {
            ns = String::from("https://") + &*ns;
        }
        let mut res = match client.get(&*format!("{}/addr/{}", ns, ring_id)).send() {
            Ok(res) => res,
            _ => {
                return String::new();
            }
        };

        let mut body: String = String::new();
        let _ = res.read_to_string(&mut body);
        match from_str(&body) {
            Ok(j) => {
                // Only if rori order
                let j: Value = j;
                if j["name"].is_null() {
                    return String::new();
                } else {
                    return j["name"].as_str().unwrap_or("").to_string();
                }
            },
            _ => {
                return String::new();
            }
        };
    }

    // Helpers

    /**
     * Add a RING account
     * @param main_info path or alias
     * @param password
     * @param from_archive if main_info is a path
     */
    pub fn add_account(main_info: &str, password: &str, from_archive: bool) {
        let mut details: HashMap<&str, &str> = HashMap::new();
        if from_archive {
            details.insert("Account.archivePath", main_info);
        } else {
            details.insert("Account.alias", main_info);
        }
        details.insert("Account.type", "RING");
        details.insert("Account.archivePassword", password);
        let details = Dict::new(details.iter());
        let dbus_msg = Message::new_method_call("cx.ring.Ring", "/cx/ring/Ring/ConfigurationManager",
                                                "cx.ring.Ring.ConfigurationManager",
                                                "addAccount");
        if !dbus_msg.is_ok() {
            error!("addAccount fails. Please verify daemon's API.");
            return;
        }
        let conn = Connection::get_private(BusType::Session);
        if !conn.is_ok() {
            return;
        }
        let dbus = conn.unwrap();
        let response = dbus.send_with_reply_and_block(dbus_msg.unwrap()
                                                                .append1(details), 2000).unwrap();
        // addAccount returns one argument, which is a string.
        let account_added: &str  = match response.get1() {
            Some(account) => account,
            None => ""
        };
        info!("New account: {:?}", account_added);
    }

    /**
     * Get current ring accounts
     * @return current accounts
     */
    pub fn get_account_list() -> Vec<Account> {
        let mut account_list: Vec<Account> = Vec::new();
        let dbus_msg = Message::new_method_call("cx.ring.Ring", "/cx/ring/Ring/ConfigurationManager",
                                                "cx.ring.Ring.ConfigurationManager",
                                                "getAccountList");
        if !dbus_msg.is_ok() {
            error!("getAccountList fails. Please verify daemon's API.");
            return account_list;
        }
        let conn = Connection::get_private(BusType::Session);
        if !conn.is_ok() {
            return account_list;
        }
        let dbus = conn.unwrap();
        let response = dbus.send_with_reply_and_block(dbus_msg.unwrap(), 2000).unwrap();
        // getAccountList returns one argument, which is an array of strings.
        let accounts: Array<&str, _>  = match response.get1() {
            Some(array) => array,
            None => return account_list
        };
        for account in accounts {
            account_list.push(Endpoint::build_account(account));
        }
        account_list
    }

// Private stuff
    /**
     * Build a new account with an id from the daemon
     * @param id the account id to build
     * @return the account retrieven
     */
    fn build_account(id: &str) -> Account {
        let dbus_msg = Message::new_method_call("cx.ring.Ring", "/cx/ring/Ring/ConfigurationManager",
                                                "cx.ring.Ring.ConfigurationManager",
                                                "getAccountDetails");
        if !dbus_msg.is_ok() {
            error!("getAccountDetails fails. Please verify daemon's API.");
            return Account::null();
        }
        let conn = Connection::get_private(BusType::Session);
        if !conn.is_ok() {
            error!("connection not ok.");
            return Account::null();
        }
        let dbus = conn.unwrap();
        let response = dbus.send_with_reply_and_block(
                                           dbus_msg.unwrap().append1(id), 2000
                                       ).ok().expect("Is the ring-daemon launched?");
        let details: Dict<&str, &str, _> = match response.get1() {
            Some(details) => details,
            None => {
                return Account::null();
            }
        };

        let mut account = Account::null();
        account.id = id.to_owned();
        for detail in details {
            match detail {
                (key, value) => {
                    if key == "Account.enable" {
                        account.enabled = value == "true";
                    }
                    if key == "Account.alias" {
                        account.alias = String::from(value);
                    }
                    if key == "Account.username" {
                        account.ring_id = String::from(value).replace("ring:", "");
                    }
                }
            }
        }
        account
    }

    /**
     * Enable a Ring account
     * @param self
     */
    pub fn enable_account(&self) {
        let dbus_msg = Message::new_method_call(self.ring_dbus, self.configuration_path,
                                                self.configuration_iface,
                                                "sendRegister");
        if !dbus_msg.is_ok() {
            error!("sendRegister call fails. Please verify daemon's API.");
            return;
        }
        let conn = Connection::get_private(BusType::Session);
        if !conn.is_ok() {
            return;
        }
        let dbus = conn.unwrap();
        let _ = dbus.send_with_reply_and_block(
            dbus_msg.unwrap().append2(self.account.id.clone(), true), 2000);
    }

    /**
     * Update current RORI account by handling accountsChanged signals from daemon.
     * @param self
     * @param ci
     */
    fn handle_accounts_signals(&mut self, ci: &ConnectionItem) {
        // Check signal
        let msg = if let &ConnectionItem::Signal(ref signal) = ci { signal } else { return };
        if &*msg.interface().unwrap() != "cx.ring.Ring.ConfigurationManager" { return };
        if &*msg.member().unwrap() != "accountsChanged" { return };
        // TODO test if RORI accounts is still exists
    }

    /**
    * Handle new interactions signals
    * @param self
    * @param ci
    * @return (accountId, interaction)
    */
    fn handle_interactions(&self, ci: &ConnectionItem) -> Option<(String, Interaction)> {
        // Check signal
        let msg = if let &ConnectionItem::Signal(ref signal) = ci { signal } else { return None };
        if &*msg.interface().unwrap() != "cx.ring.Ring.ConfigurationManager" { return None };
        if &*msg.member().unwrap() != "incomingAccountMessage" { return None };
        // incomingAccountMessage return three arguments
        let (account_id, author_ring_id, payloads) = msg.get3::<&str, &str, Dict<&str, &str, _>>();
        let author_ring_id = author_ring_id.unwrap().to_string();
        let mut body = String::new();
        let mut datatype = String::new();
        let mut metadatas: HashMap<String, String> = HashMap::new();
        for detail in payloads.unwrap() {
            match detail {
                (key, value) => {
                    // TODO for now, text/plain is the only supported datatypes, changes this with key in supported datatypes
                    if key == "text/plain" || key == "rori/message" {
                        datatype = key.to_string();
                        body = value.to_string();
                    } else {
                        metadatas.insert(
                            key.to_string(),
                            value.to_string()
                        );
                    }
                }
            }
        };
        let interaction = Interaction {
            author_ring_id: author_ring_id,
            body: body,
            datatype: datatype,
            time: time::now(),
            metadatas: metadatas
        };
        Some((account_id.unwrap().to_string(), interaction))
    }

    /**
     * Update current RORI account by handling accountsChanged signals from daemon
     * @param self
     * @param ci
     */
    fn handle_registration_changed(&self, ci: &ConnectionItem) {
        // Check signal
        let msg = if let &ConnectionItem::Signal(ref signal) = ci { signal } else { return };
        if &*msg.interface().unwrap() != "cx.ring.Ring.ConfigurationManager" { return };
        if &*msg.member().unwrap() != "registrationStateChanged" { return };
        // let (account_id, registration_state, _, _) = msg.get4::<&str, &str, u64, &str>();
        // TODO the account can be disabled. Inform UI
    }

    /**
     * Handle new pending requests signals
     * @param self
     * @param ci
     * @return (accountId, from)
     */
    fn handle_requests(&self, ci: &ConnectionItem) -> Option<(String, String)> {
        // Check signal
        let msg = if let &ConnectionItem::Signal(ref signal) = ci { signal } else { return None };
        if &*msg.interface().unwrap() != "cx.ring.Ring.ConfigurationManager" { return None };
        if &*msg.member().unwrap() != "incomingTrustRequest" { return None };
        // incomingTrustRequest return three arguments
        let (account_id, from, _, _) = msg.get4::<&str, &str, Dict<&str, &str, _>, u64>();
        Some((account_id.unwrap().to_string(), from.unwrap().to_string()))
    }

    /**
     * Detect if a message is a correct command
     * Based on https://github.com/AmarOk1412/rori_core/wiki/Custom-datatypes-handling
     * NOTE: some commands are forbidden user side (like datatypes management)
     * @param self
     * @param text to verify
     * @return true if it's a correct command
     */
    fn is_a_command(&self, text: &String) -> bool {
        let v: Vec<&str> = text.split(' ').collect();
        if v.len() == 0 {
            return false
        }
        let whitelist_commands = ["/register", "/unregister",
                                  "/add_device", "/rm_device", "/link"];
        whitelist_commands.contains(&v[0])
    }


    /**
     * Send a new text message to rori
     * @param self
     * @param body text to send
     * @return the interaction id if success. TODO, watch message status (if received)
     */
    fn send_interaction_to_rori(&self, payloads: HashMap<&str, &str>) -> u64 {
        let payloads = Dict::new(payloads.iter());

        let dbus_msg = Message::new_method_call(self.ring_dbus, self.configuration_path, self.configuration_iface,
                                                "sendTextMessage");
        if !dbus_msg.is_ok() {
            error!("sendTextMessage fails. Please verify daemon's API.");
            return 0;
        }
        let conn = Connection::get_private(BusType::Session);
        if !conn.is_ok() {
            return 0;
        }
        let dbus = conn.unwrap();
        let response = dbus.send_with_reply_and_block(dbus_msg.unwrap().append3(&*self.account.id,
            self.rori_ring_id.clone(), payloads), 2000).unwrap();
        // sendTextMessage returns one argument, which is a u64.
        let interaction_id: u64  = match response.get1() {
            Some(interaction_id) => interaction_id,
            None => 0
        };
        interaction_id
    }
}
