use super::{App, User};
use anyhow::Result;
use rusqlite::OptionalExtension;

impl App {
    pub fn require_login(&self) -> Result<bool> {
        let value: Option<String> = self
            .conn()?
            .query_row(
                "SELECT value FROM settings WHERE key='require_login'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value.as_deref() == Some("true"))
    }

    pub fn set_require_login(&self, actor: &User, required: bool) -> Result<()> {
        self.require_admin(actor)?;
        self.conn()?.execute(
            "INSERT INTO settings(key,value) VALUES('require_login',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [if required { "true" } else { "false" }],
        )?;
        Ok(())
    }
}
