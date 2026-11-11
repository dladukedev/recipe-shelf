use super::{App, Recipe, RecipePage};
use anyhow::Result;
use rusqlite::OptionalExtension;

impl App {
    pub fn recipes(&self, query: &str, limit: usize, offset: usize) -> Result<Vec<Recipe>> {
        // Keep the read-only API's documented full-TOML search contract.
        let needle = query.to_lowercase();
        Ok(self
            .export_recipes()?
            .into_iter()
            .filter(|r| r.raw_toml.to_lowercase().contains(&needle))
            .skip(offset)
            .take(limit.min(500))
            .collect())
    }
    pub fn recipe_page(
        &self,
        query: &str,
        category: &str,
        sort: &str,
        limit: usize,
        offset: usize,
    ) -> Result<RecipePage> {
        let all = self.export_recipes()?;
        let mut categories: Vec<String> = all
            .iter()
            .flat_map(|r| r.data.categories.iter().cloned())
            .collect();
        categories.sort_by_key(|c| c.to_lowercase());
        categories.dedup();
        let needle = query.trim().to_lowercase();
        let mut items: Vec<Recipe> = all
            .into_iter()
            .filter(|r| {
                (category.is_empty() || r.data.categories.iter().any(|c| c == category))
                    && (r.data.title.to_lowercase().contains(&needle)
                        || r.data.description.to_lowercase().contains(&needle))
            })
            .collect();
        if sort == "time" {
            items.sort_by_key(|r| {
                (
                    r.data.total_time.is_none(),
                    r.data.total_time.unwrap_or_default(),
                )
            });
        }
        let total = items.len();
        Ok(RecipePage {
            items: items
                .into_iter()
                .skip(offset)
                .take(limit.min(500))
                .collect(),
            total,
            categories,
        })
    }
    pub fn recipe(&self, id: i64) -> Result<Option<Recipe>> {
        let db = self.conn()?;
        let row: Option<(String, String)> = db
            .query_row(
                "SELECT data_json,raw_toml FROM recipes WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(j, raw)| {
            Ok(Recipe {
                id,
                data: serde_json::from_str(&j)?,
                raw_toml: raw,
            })
        })
        .transpose()
    }
    pub fn export_recipes(&self) -> Result<Vec<Recipe>> {
        let db = self.conn()?;
        let mut st = db.prepare(
            "SELECT id,data_json,raw_toml FROM recipes ORDER BY title COLLATE NOCASE,id",
        )?;
        Ok(st
            .query_map([], |r| {
                let json: String = r.get(1)?;
                Ok(Recipe {
                    id: r.get(0)?,
                    data: serde_json::from_str(&json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    raw_toml: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
