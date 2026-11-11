use super::{App, Login, User};
use crate::domain_error::{DomainError, account_write_error};
use anyhow::{Context, Result, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rand::rngs::OsRng;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::sync::OnceLock;

static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();

impl App {
    pub fn needs_setup(&self) -> Result<bool> {
        Ok(self
            .conn()?
            .query_row("SELECT NOT EXISTS(SELECT 1 FROM users)", [], |r| {
                r.get::<_, bool>(0)
            })?)
    }
    pub fn setup(&self, username: &str, password: &str) -> Result<User> {
        validate_user(username, password)?;
        let hash = hash_password(password)?;
        let mut db = self.conn()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM users)", [], |r| r.get(0))?;
        if exists {
            return Err(DomainError::Conflict("setup has already been completed").into());
        };
        tx.execute(
            "INSERT INTO users(username,password_hash,is_admin) VALUES(?1,?2,1)",
            params![username.trim(), hash],
        )
        .map_err(account_write_error)
        .context("creating initial administrator")?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(User {
            id,
            username: username.trim().into(),
            is_admin: true,
        })
    }
    pub fn login(&self, username: &str, password: &str) -> Result<Login> {
        let user = self.verify_credentials(username, password)?;
        self.issue_session(user)
    }

    fn verify_credentials(&self, username: &str, password: &str) -> Result<User> {
        if password.len() > 1024 {
            return Err(DomainError::InvalidCredentials.into());
        }
        let row: Option<(i64, String, bool, String)> = {
            let db = self.conn()?;
            db.query_row("SELECT id,username,is_admin,password_hash FROM users WHERE username=?1 COLLATE NOCASE",[username.trim()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?
        };
        let Some((id, name, admin, hash)) = row else {
            let dummy = DUMMY_PASSWORD_HASH.get_or_init(|| {
                hash_password("recipe-shelf timing equalizer password")
                    .expect("generating process dummy password hash")
            });
            if let Ok(parsed) = PasswordHash::new(dummy) {
                let _ = Argon2::default().verify_password(password.as_bytes(), &parsed);
            }
            return Err(DomainError::InvalidCredentials.into());
        };
        let parsed = PasswordHash::new(&hash).context("invalid stored password hash")?;
        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_err()
        {
            return Err(DomainError::InvalidCredentials.into());
        };
        Ok(User {
            id,
            username: name,
            is_admin: admin,
        })
    }

    pub fn create_user(
        &self,
        actor: &User,
        username: &str,
        password: &str,
        is_admin: bool,
    ) -> Result<User> {
        self.require_admin(actor)?;
        validate_user(username, password)?;
        let hash = hash_password(password)?;
        let db = self.conn()?;
        db.execute(
            "INSERT INTO users(username,password_hash,is_admin) VALUES(?1,?2,?3)",
            params![username.trim(), hash, is_admin],
        )
        .map_err(account_write_error)?;
        Ok(User {
            id: db.last_insert_rowid(),
            username: username.trim().into(),
            is_admin,
        })
    }
    pub fn update_user(
        &self,
        actor: &User,
        id: i64,
        username: &str,
        password: Option<&str>,
        is_admin: bool,
    ) -> Result<()> {
        self.require_admin(actor)?;
        validate_user(username, password.unwrap_or("validation-placeholder"))?;
        if id == actor.id && (!is_admin || password.is_some()) {
            return Err(DomainError::InvalidInput(
                "use your account password form; another administrator must change your role",
            )
            .into());
        }
        let hash = password.map(hash_password).transpose()?;
        let mut db = self.conn()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_transaction_admin(&tx, actor.id)?;
        let was_admin: bool = tx
            .query_row("SELECT is_admin FROM users WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or(DomainError::NotFound("user not found"))?;
        if was_admin && !is_admin {
            let admins: i64 =
                tx.query_row("SELECT count(*) FROM users WHERE is_admin=1", [], |r| {
                    r.get(0)
                })?;
            if admins <= 1 {
                return Err(DomainError::Conflict("keep at least one administrator").into());
            }
        }
        tx.execute(
            "UPDATE users SET username=?1,is_admin=?2 WHERE id=?3",
            params![username.trim(), is_admin, id],
        )
        .map_err(account_write_error)?;
        if let Some(hash) = hash {
            tx.execute(
                "UPDATE users SET password_hash=?1 WHERE id=?2",
                params![hash, id],
            )?;
        }
        if password.is_some() || was_admin != is_admin {
            tx.execute("DELETE FROM sessions WHERE user_id=?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn delete_user(&self, actor: &User, id: i64) -> Result<()> {
        if actor.id == id {
            return Err(DomainError::InvalidInput("you cannot delete your own account").into());
        }
        let mut db = self.conn()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_transaction_admin(&tx, actor.id)?;
        let admin: bool = tx
            .query_row("SELECT is_admin FROM users WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or(DomainError::NotFound("user not found"))?;
        let admins: i64 = tx.query_row("SELECT count(*) FROM users WHERE is_admin=1", [], |r| {
            r.get(0)
        })?;
        if admin && admins <= 1 {
            return Err(DomainError::Conflict("keep at least one administrator").into());
        }
        tx.execute("DELETE FROM users WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn change_password(&self, actor: &User, current: &str, new: &str) -> Result<()> {
        validate_user(&actor.username, new)?;
        if current.len() > 1024 || current == new {
            return Err(DomainError::InvalidInput(
                "check your current password and choose a different new password",
            )
            .into());
        }
        let previous: String = self.conn()?.query_row(
            "SELECT password_hash FROM users WHERE id=?1",
            [actor.id],
            |r| r.get(0),
        )?;
        let parsed =
            PasswordHash::new(&previous).map_err(|_| anyhow!("invalid stored password"))?;
        Argon2::default()
            .verify_password(current.as_bytes(), &parsed)
            .map_err(|_| DomainError::InvalidInput("current password is incorrect"))?;
        let hash = hash_password(new)?;
        let mut db = self.conn()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute(
            "UPDATE users SET password_hash=?1 WHERE id=?2 AND password_hash=?3",
            params![hash, actor.id, previous],
        )? != 1
        {
            return Err(
                DomainError::Conflict("password changed concurrently; sign in again").into(),
            );
        }
        tx.execute("DELETE FROM sessions WHERE user_id=?1", [actor.id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn users(&self, actor: &User) -> Result<Vec<User>> {
        self.require_admin(actor)?;
        let db = self.conn()?;
        let mut st =
            db.prepare("SELECT id,username,is_admin FROM users ORDER BY username COLLATE NOCASE")?;
        Ok(st
            .query_map([], |r| {
                Ok(User {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    is_admin: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub(super) fn require_admin(&self, actor: &User) -> Result<()> {
        let real: Option<bool> = self
            .conn()?
            .query_row("SELECT is_admin FROM users WHERE id=?1", [actor.id], |r| {
                r.get(0)
            })
            .optional()?;
        if real != Some(true) {
            return Err(DomainError::Forbidden("administrator access required").into());
        };
        Ok(())
    }
}

fn require_transaction_admin(tx: &rusqlite::Transaction<'_>, id: i64) -> Result<()> {
    let admin = tx
        .query_row("SELECT is_admin FROM users WHERE id=?1", [id], |r| {
            r.get::<_, bool>(0)
        })
        .optional()?;
    if admin != Some(true) {
        return Err(DomainError::Forbidden("administrator access required").into());
    }
    Ok(())
}
fn validate_user(u: &str, p: &str) -> Result<()> {
    let u = u.trim();
    if u.is_empty() || u.chars().count() > 64 {
        return Err(DomainError::InvalidInput("username must contain 1 to 64 characters").into());
    };
    if !(8..=1024).contains(&p.len()) {
        return Err(DomainError::InvalidInput("password must contain 8 to 1024 bytes").into());
    };
    Ok(())
}
fn hash_password(p: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(p.as_bytes(), &salt)?
        .to_string())
}
