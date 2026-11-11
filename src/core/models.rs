use super::parsing::deserialize_minutes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeData {
    pub title: String,
    pub description: String,
    pub ingredients: String,
    pub instructions: Vec<String>,
    pub notes: Vec<String>,
    pub categories: Vec<String>,
    pub cuisines: Vec<String>,
    pub author: String,
    pub r#yield: String,
    #[serde(
        default,
        deserialize_with = "deserialize_minutes",
        skip_serializing_if = "Option::is_none"
    )]
    pub prep_time: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_minutes",
        skip_serializing_if = "Option::is_none"
    )]
    pub cook_time: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_minutes",
        skip_serializing_if = "Option::is_none"
    )]
    pub total_time: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    pub id: i64,
    pub data: RecipeData,
    pub raw_toml: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub recipe_count: usize,
    pub last_sync: Option<FolderSync>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FolderSync {
    pub completed_at: i64,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub errors: Vec<String>,
}
pub struct RecipePage {
    pub items: Vec<Recipe>,
    pub total: usize,
    pub categories: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Login {
    pub token: String,
    pub user: User,
    pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct SyncReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub errors: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ExportTemplates {
    pub text: String,
    pub markdown: String,
    pub pdf: String,
}
