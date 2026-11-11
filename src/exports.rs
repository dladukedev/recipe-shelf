use std::fmt;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{
    core::{App, ExportTemplates, Recipe},
    mustache,
};

const MAX_EXPORT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug)]
pub enum ExportError {
    InvalidFormat,
    RecipeNotFound,
    PdfUnavailable,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidFormat => "Choose text, markdown, or pdf.",
            Self::RecipeNotFound => "Recipe not found.",
            Self::PdfUnavailable => "PDF rendering is unavailable. Install WeasyPrint for the server's Python interpreter, or use the supplied container image.",
        })
    }
}

impl std::error::Error for ExportError {}

pub struct ExportFile {
    pub filename: String,
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Text,
    Markdown,
    Pdf,
}

impl Format {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Markdown => "markdown",
            Self::Pdf => "pdf",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "text" => Ok(Self::Text),
            "markdown" => Ok(Self::Markdown),
            "pdf" => Ok(Self::Pdf),
            _ => Err(ExportError::InvalidFormat.into()),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Markdown => "md",
            Self::Pdf => "pdf",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Text => "text/plain; charset=utf-8",
            Self::Markdown => "text/markdown; charset=utf-8",
            Self::Pdf => "application/pdf",
        }
    }

    pub(crate) fn template(self, templates: &ExportTemplates) -> &str {
        match self {
            Self::Text => &templates.text,
            Self::Markdown => &templates.markdown,
            Self::Pdf => &templates.pdf,
        }
    }
}

pub fn default_templates() -> ExportTemplates {
    ExportTemplates {
        text: include_str!("../templates/recipe.txt.mustache").into(),
        markdown: include_str!("../templates/recipe.md.mustache").into(),
        pdf: include_str!("../templates/recipe.html.mustache").into(),
    }
}

pub fn format_minutes(minutes: u64) -> String {
    let hours = minutes / 60;
    let remainder = minutes % 60;
    match (hours, remainder) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} {}", if hours == 1 { "hr" } else { "hrs" }),
        (hours, minutes) => format!(
            "{hours} {} {minutes} min",
            if hours == 1 { "hr" } else { "hrs" }
        ),
    }
}

pub fn recipe_context(recipe: &Recipe) -> Result<Value> {
    let mut context = serde_json::to_value(&recipe.data)?;
    let object = context
        .as_object_mut()
        .context("recipe data must be an object")?;
    object.insert("id".into(), json!(recipe.id));
    object.insert("raw_toml".into(), json!(recipe.raw_toml));
    object.insert(
        "ingredient_lines".into(),
        json!(recipe.data.ingredients.lines().collect::<Vec<_>>()),
    );
    object.insert(
        "has_instructions".into(),
        json!(!recipe.data.instructions.is_empty()),
    );
    object.insert(
        "steps".into(),
        json!(
            recipe
                .data
                .instructions
                .iter()
                .enumerate()
                .map(|(i, text)| json!({"number": i + 1, "text": text}))
                .collect::<Vec<_>>()
        ),
    );
    for (field, minutes_field, value) in [
        ("prep_time", "prep_minutes", recipe.data.prep_time),
        ("cook_time", "cook_minutes", recipe.data.cook_time),
        ("total_time", "total_minutes", recipe.data.total_time),
    ] {
        object.insert(
            field.into(),
            value.map_or(Value::Null, |n| json!(format_minutes(n))),
        );
        object.insert(
            minutes_field.into(),
            value.map_or(Value::Null, |n| json!(n)),
        );
    }
    Ok(context)
}

fn filename(recipe: &Recipe, format: Format) -> String {
    let mut slug = String::new();
    for character in recipe.data.title.chars() {
        if slug.len() >= 80 {
            break;
        }
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    format!(
        "{}-{}.{}",
        if slug.is_empty() { "recipe" } else { slug },
        recipe.id,
        format.extension()
    )
}

pub fn export_one(app: &App, id: i64, format: &str) -> Result<ExportFile> {
    let format = Format::parse(format)?;
    let recipe = app.recipe(id)?.ok_or(ExportError::RecipeNotFound)?;
    let templates = app.export_templates()?;
    let rendered = mustache::render(format.template(&templates), &recipe_context(&recipe)?)?;
    let bytes = match format {
        Format::Pdf => crate::pdf::render_documents(&[rendered])?
            .into_iter()
            .next()
            .context("PDF renderer returned no document")?,
        _ => rendered.into_bytes(),
    };
    Ok(ExportFile {
        filename: filename(&recipe, format),
        content_type: format.content_type(),
        bytes,
    })
}

pub fn export_all(app: &App, format: &str) -> Result<ExportFile> {
    let format = Format::parse(format)?;
    let templates = app.export_templates()?;
    let recipes = app.export_recipes()?;
    if recipes.len() > u16::MAX as usize {
        bail!("Too many recipes for a single ZIP export.");
    }
    let mut rendered = Vec::with_capacity(recipes.len());
    let mut size = 0usize;
    for recipe in &recipes {
        let document = mustache::render(format.template(&templates), &recipe_context(recipe)?)?;
        size = size
            .checked_add(document.len())
            .context("Export size overflow")?;
        if size > 64 * 1024 * 1024 {
            bail!("Export exceeds the 64 MiB rendered document limit.");
        }
        rendered.push(document);
    }
    let documents = match format {
        Format::Pdf if !rendered.is_empty() => crate::pdf::render_documents(&rendered)?,
        _ => rendered.into_iter().map(String::into_bytes).collect(),
    };
    if documents.len() != recipes.len() {
        bail!("PDF renderer returned an incomplete collection.");
    }
    let entries: Vec<_> = recipes
        .iter()
        .zip(documents)
        .map(|(recipe, bytes)| (filename(recipe, format), bytes))
        .collect();
    Ok(ExportFile {
        filename: format!("recipes-{}.zip", format.extension()),
        content_type: "application/zip",
        bytes: zip_archive(&entries)?,
    })
}

// ZIP's uncompressed storage method keeps exports portable and avoids a runtime
// archiver. Names are generated internally and contain only ASCII letters,
// digits, hyphens, and the extension; IDs prevent duplicate titles colliding.
fn zip_archive(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    if entries.len() > u16::MAX as usize {
        bail!("Too many ZIP entries.");
    }
    let mut output = Vec::new();
    let mut directory = Vec::new();
    for (name, contents) in entries {
        if output
            .len()
            .saturating_add(contents.len())
            .saturating_add(name.len() + 30)
            > MAX_EXPORT_BYTES
        {
            bail!("Export exceeds the 128 MiB archive limit.");
        }
        let offset = u32::try_from(output.len())?;
        let length = u32::try_from(contents.len())?;
        let name_length = u16::try_from(name.len())?;
        let crc = crc32fast::hash(contents);
        output.extend_from_slice(&0x04034b50u32.to_le_bytes());
        for number in [20u16, 0, 0, 0, 33] {
            output.extend_from_slice(&number.to_le_bytes());
        }
        for number in [crc, length, length] {
            output.extend_from_slice(&number.to_le_bytes());
        }
        for number in [name_length, 0] {
            output.extend_from_slice(&number.to_le_bytes());
        }
        output.extend_from_slice(name.as_bytes());
        output.extend_from_slice(contents);

        directory.extend_from_slice(&0x02014b50u32.to_le_bytes());
        for number in [20u16, 20, 0, 0, 0, 33] {
            directory.extend_from_slice(&number.to_le_bytes());
        }
        for number in [crc, length, length] {
            directory.extend_from_slice(&number.to_le_bytes());
        }
        for number in [name_length, 0, 0, 0, 0] {
            directory.extend_from_slice(&number.to_le_bytes());
        }
        for number in [0u32, offset] {
            directory.extend_from_slice(&number.to_le_bytes());
        }
        directory.extend_from_slice(name.as_bytes());
    }
    let directory_offset = u32::try_from(output.len())?;
    let directory_size = u32::try_from(directory.len())?;
    if output.len().saturating_add(directory.len() + 22) > MAX_EXPORT_BYTES {
        bail!("Export exceeds the 128 MiB archive limit.");
    }
    output.extend_from_slice(&directory);
    output.extend_from_slice(&0x06054b50u32.to_le_bytes());
    for number in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        output.extend_from_slice(&number.to_le_bytes());
    }
    for number in [directory_size, directory_offset] {
        output.extend_from_slice(&number.to_le_bytes());
    }
    output.extend_from_slice(&0u16.to_le_bytes());
    Ok(output)
}
