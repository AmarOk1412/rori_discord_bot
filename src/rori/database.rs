/**
 * Copyright (c) 2019, Sébastien Blin <sebastien.blin@enconn.fr>
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

use rusqlite;

/**
 * This class furnish helpers to manipulate the rori.db sqlite database
 */
pub struct Database;

impl Database {

    /**
     * Insert new user
     * @param id the id of this user
     * @param username username linked
     * @return the line's id inserted if success, else an error
     */
    pub fn add_user(id: &String, username: &String) -> Result<i32, rusqlite::Error> {
        let conn = rusqlite::Connection::open("rori_discord_bot.db").unwrap();
        let mut conn = conn.prepare("INSERT INTO usernames (id, username)
                                     VALUES (:id, :username)").unwrap();
        conn.execute_named(&[(":id", id), (":username", username)])
    }

    /**
     * Create tables in rori.db
     * NOTE: maybe has to change in case of migrations
     */
    pub fn init_db() {
        let conn = rusqlite::Connection::open("rori_discord_bot.db").unwrap();
        let mut q = conn.prepare("PRAGMA user_version").unwrap();
        let version: i64 = q.query_row(&[], |row| row.get(0)).unwrap_or(0);
        let mut do_migration = true;
        if version == 1 {
            do_migration = false;
        }
        if do_migration {
            info!("migrate database to version 1");
            conn.execute("CREATE TABLE IF NOT EXISTS usernames (
                id               TEXT PRIMARY KEY,
                username         TEXT
                )", &[]).unwrap();
            conn.execute("PRAGMA user_version = 1", &[]).unwrap();
        }
        info!("database ready");
    }

    /**
     * Retrieve username from id
     * @param id of the device
     * @return username or empty String
     */
    pub fn username(id: &String) -> String {
        let conn = rusqlite::Connection::open("rori_discord_bot.db").unwrap();
        let mut stmt = conn.prepare("SELECT username FROM usernames WHERE id=:id").unwrap();
        let mut rows = stmt.query_named(&[(":id", id)]).unwrap();
        if let Some(row) = rows.next() {
            let row = row.unwrap();
            return row.get(0);
        }
        String::new()
    }

    /**
     * Retrieve id from username
     * @param username of the device
     * @return id or empty String
     */
    pub fn id(username: &String) -> String {
        let conn = rusqlite::Connection::open("rori_discord_bot.db").unwrap();
        let mut stmt = conn.prepare("SELECT id FROM usernames WHERE username=:username").unwrap();
        let mut rows = stmt.query_named(&[(":username", username)]).unwrap();
        if let Some(row) = rows.next() {
            let row = row.unwrap();
            return row.get(0);
        }
        String::new()
    }

    /**
     * Remove a user from the usernames table
     * @param username to remove
     * @return the id of the removed row or an error
     */
    pub fn remove_user(username: &String) -> Result<i32, rusqlite::Error> {
        let conn = rusqlite::Connection::open("rori_discord_bot.db").unwrap();
        let mut conn = conn.prepare("DELETE FROM usernames WHERE username=:username").unwrap();
        conn.execute_named(&[(":username", username)])
    }
}
