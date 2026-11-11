use super::{App, ExportTemplates, User};
use crate::domain_error::DomainError;
use anyhow::Result;
use rusqlite::params;

impl App {
    pub fn export_templates(&self) -> Result<ExportTemplates> {
        let mut templates = crate::exports::default_templates();
        let db = self.conn()?;
        let mut st = db.prepare("SELECT format,source FROM export_templates")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (format, source) = row?;
            match format.as_str() {
                "text" => templates.text = source,
                "markdown" => templates.markdown = source,
                "pdf" => templates.pdf = source,
                _ => {}
            }
        }
        Ok(templates)
    }
    pub fn save_export_template(&self, actor: &User, format: &str, source: &str) -> Result<()> {
        self.require_admin(actor)?;
        if !matches!(format, "text" | "markdown" | "pdf") {
            return Err(DomainError::InvalidInput("format must be text, markdown, or pdf").into());
        }
        crate::mustache::validate(source)?;
        let db = self.conn()?;
        let tx = db.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO export_templates(format,source) VALUES(?1,?2) ON CONFLICT(format) DO UPDATE SET source=excluded.source",
            params![format, source],
        )?;
        tx.commit()?;
        Ok(())
    }
}
