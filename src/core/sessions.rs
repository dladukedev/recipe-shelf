use super::{App, Login, TOKEN_TTL, User, storage::now};
use anyhow::Result;
use rand::{RngCore, rngs::OsRng};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

impl App {
    pub(super) fn issue_session(&self, user: User) -> Result<Login> {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        let expires = now()?.saturating_add(TOKEN_TTL);
        let db = self.conn()?;
        let tx = db.unchecked_transaction()?;
        tx.execute("DELETE FROM sessions WHERE expires_at<=?1", [now()?])?;
        tx.execute(
            "INSERT INTO sessions(token_hash,user_id,expires_at) VALUES(?1,?2,?3)",
            params![token_hash(&token), user.id, expires],
        )?;
        tx.commit()?;
        Ok(Login {
            token,
            user,
            expires_at: expires,
        })
    }
    pub fn authenticate(&self, token: &str) -> Result<Option<User>> {
        let db = self.conn()?;
        let row=db.query_row("SELECT u.id,u.username,u.is_admin FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=?1 AND s.expires_at>?2",params![token_hash(token),now()?],|r|Ok(User{id:r.get(0)?,username:r.get(1)?,is_admin:r.get(2)?})).optional()?;
        Ok(row)
    }
    pub fn logout(&self, token: &str) -> Result<()> {
        self.conn()?.execute(
            "DELETE FROM sessions WHERE token_hash=?1",
            [token_hash(token)],
        )?;
        Ok(())
    }
}

pub(super) fn token_hash(t: &str) -> String {
    hex::encode(Sha256::digest(t.as_bytes()))
}
