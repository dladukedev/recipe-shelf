use super::{App, Folder, FolderSync, SyncReport, User, parsing::read_recipe, storage::now};
use crate::domain_error::DomainError;
use anyhow::{Result, anyhow};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

impl App {
    pub fn folders(&self, actor: &User) -> Result<Vec<Folder>> {
        self.require_admin(actor)?;
        let db = self.conn()?;
        let mut st = db.prepare("SELECT f.id,f.path,(SELECT count(*) FROM recipes r WHERE r.folder_id=f.id),s.report_json FROM folders f LEFT JOIN folder_sync s ON s.folder_id=f.id ORDER BY f.path")?;
        Ok(st
            .query_map([], |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    recipe_count: r.get(2)?,
                    last_sync: r
                        .get::<_, Option<String>>(3)?
                        .map(|json| serde_json::from_str(&json))
                        .transpose()
                        .map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn add_folder(&self, actor: &User, path: &str) -> Result<Folder> {
        self.require_admin(actor)?;
        let canonical = Path::new(path)
            .canonicalize()
            .map_err(|_| DomainError::InvalidInput("folder must be an existing directory"))?;
        if !canonical.is_dir() {
            return Err(DomainError::InvalidInput("folder must be a directory").into());
        };
        let stored = canonical.to_string_lossy().into_owned();
        let db = self.conn()?;
        let mut st = db.prepare("SELECT path FROM folders")?;
        let paths = st
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for p in paths {
            let old = PathBuf::from(p);
            if canonical.starts_with(&old) || old.starts_with(&canonical) {
                return Err(DomainError::Conflict("folder overlaps an existing folder").into());
            }
        }
        db.execute("INSERT INTO folders(path) VALUES(?1)", [&stored])?;
        Ok(Folder {
            id: db.last_insert_rowid(),
            path: stored,
            recipe_count: 0,
            last_sync: None,
        })
    }
    pub fn remove_folder(&self, actor: &User, id: i64) -> Result<()> {
        self.require_admin(actor)?;
        let _sync = self
            .sync_lock
            .lock()
            .map_err(|_| anyhow!("sync lock poisoned"))?;
        if self
            .conn()?
            .execute("DELETE FROM folders WHERE id=?1", [id])?
            != 1
        {
            return Err(DomainError::NotFound("folder not found").into());
        }
        Ok(())
    }
    fn record_folder_sync(&self, id: i64, report: &FolderSync) -> Result<()> {
        self.conn()?.execute("INSERT INTO folder_sync(folder_id,report_json) VALUES(?1,?2) ON CONFLICT(folder_id) DO UPDATE SET report_json=excluded.report_json", params![id,serde_json::to_string(report)?])?;
        Ok(())
    }
    pub fn sync(&self, actor: &User) -> Result<SyncReport> {
        self.require_admin(actor)?;
        let _sync = self
            .sync_lock
            .lock()
            .map_err(|_| anyhow!("sync lock poisoned"))?;
        let folders = {
            let db = self.conn()?;
            let mut st = db.prepare("SELECT id,path FROM folders ORDER BY id")?;
            st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut report = SyncReport {
            added: 0,
            updated: 0,
            removed: 0,
            errors: vec![],
        };
        for (folder_id, root_s) in folders {
            let before = (
                report.added,
                report.updated,
                report.removed,
                report.errors.len(),
            );
            let root = PathBuf::from(&root_s);
            if std::fs::symlink_metadata(&root)
                .map(|md| md.file_type().is_symlink() || !md.is_dir())
                .unwrap_or(true)
            {
                report.errors.push(format!(
                    "{}: folder unavailable or symlinked",
                    root.display()
                ));
                self.record_folder_sync(
                    folder_id,
                    &FolderSync {
                        completed_at: now()?,
                        added: 0,
                        updated: 0,
                        removed: 0,
                        errors: report.errors[before.3..].to_vec(),
                    },
                )?;
                continue;
            }
            let mut seen = std::collections::HashSet::new();
            let mut complete = true;
            let mut parsed = Vec::new();
            for ent in WalkDir::new(&root).follow_links(false) {
                let ent = match ent {
                    Ok(e) => e,
                    Err(e) => {
                        complete = false;
                        report.errors.push(format!("{}: {e}", root.display()));
                        continue;
                    }
                };
                if !ent.file_type().is_file()
                    || !ent
                        .path()
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
                {
                    continue;
                }
                let p = ent.path().to_string_lossy().into_owned();
                seen.insert(p.clone());
                match read_recipe(ent.path()) {
                    Ok((data, raw)) => parsed.push((p, data, raw)),
                    Err(e) => report.errors.push(format!("{p}: {e}")),
                }
            }
            let mut db = self.conn()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (p, data, raw) in parsed {
                let existing: Option<(i64, String)> = tx
                    .query_row("SELECT id,raw_toml FROM recipes WHERE path=?1", [&p], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                    .optional()?;
                let json = serde_json::to_string(&data)?;
                if let Some((id, old)) = existing {
                    if old != raw {
                        tx.execute("UPDATE recipes SET folder_id=?1,title=?2,raw_toml=?3,data_json=?4 WHERE id=?5",params![folder_id,data.title,raw,json,id])?;
                        report.updated += 1
                    }
                } else {
                    tx.execute("INSERT INTO recipes(folder_id,path,title,raw_toml,data_json) VALUES(?1,?2,?3,?4,?5)",params![folder_id,p,data.title,raw,json])?;
                    report.added += 1
                }
            }
            if complete {
                let mut st = tx.prepare("SELECT path FROM recipes WHERE folder_id=?1")?;
                let existing = st
                    .query_map([folder_id], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                drop(st);
                for p in existing {
                    if !seen.contains(&p) {
                        tx.execute("DELETE FROM recipes WHERE path=?1", [p])?;
                        report.removed += 1
                    }
                }
            }
            tx.commit()?;
            drop(db);
            self.record_folder_sync(
                folder_id,
                &FolderSync {
                    completed_at: now()?,
                    added: report.added - before.0,
                    updated: report.updated - before.1,
                    removed: report.removed - before.2,
                    errors: report.errors[before.3..].to_vec(),
                },
            )?;
        }
        Ok(report)
    }
}
