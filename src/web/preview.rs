//! Shared, escaped-as-text previews for saved exports and editable templates.
use crate::{core::Recipe, domain_error::DomainError};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct Preview {
    pub preview: String,
    pub error: Option<String>,
    pub truncated: bool,
}

pub(super) fn sample(recipes: &[Recipe], id: Option<i64>) -> anyhow::Result<Option<&Recipe>> {
    match id {
        Some(id) => recipes
            .iter()
            .find(|recipe| recipe.id == id)
            .map(Some)
            .ok_or_else(|| DomainError::NotFound("recipe not found").into()),
        None => Ok(recipes.first()),
    }
}

pub(super) fn render(source: &str, recipe: Option<&Recipe>) -> anyhow::Result<Preview> {
    let Some(recipe) = recipe else {
        return Ok(Preview {
            preview: String::new(),
            error: Some("Add and sync a recipe folder to preview templates.".into()),
            truncated: false,
        });
    };
    let context = crate::exports::recipe_context(recipe)?;
    Ok(match crate::mustache::render(source, &context) {
        Ok(text) => Preview {
            truncated: text.chars().count() > 32768,
            preview: text.chars().take(32768).collect(),
            error: None,
        },
        Err(error) => Preview {
            preview: String::new(),
            error: Some(format!("Template preview: {error}")),
            truncated: false,
        },
    })
}

pub(super) fn http_error(error: anyhow::Error) -> topcoat::Error {
    if matches!(
        error.downcast_ref::<DomainError>(),
        Some(DomainError::NotFound(_))
    ) {
        topcoat::router::error::not_found().into()
    } else {
        super::server_error(error)
    }
}
