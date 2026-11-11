use super::{MAX_RECIPE_BYTES, RecipeData};
use anyhow::{Context, Result, anyhow, bail};
use serde::de::{self, Visitor};
use std::{io::Read, path::Path};

pub(super) fn deserialize_minutes<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct MinutesVisitor;
    impl<'de> Visitor<'de> for MinutesVisitor {
        type Value = Option<u64>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a nonnegative integer minute count or an H/M duration string")
        }

        fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
            Ok(Some(value))
        }
        fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
            u64::try_from(value)
                .map(Some)
                .map_err(|_| E::custom("duration minutes cannot be negative"))
        }
        fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
            parse_duration_minutes(value).map_err(E::custom)
        }
        fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Self::Value, E> {
            self.visit_str(&value)
        }
    }
    deserializer.deserialize_any(MinutesVisitor)
}

fn parse_duration_minutes(value: &str) -> Result<Option<u64>> {
    let input = value.trim();
    if input.is_empty() {
        return Ok(None);
    }
    if input.bytes().all(|b| b.is_ascii_digit()) {
        return input
            .parse::<u64>()
            .map(Some)
            .context("minute count overflows u64");
    }
    let body = input
        .strip_prefix("PT")
        .ok_or_else(|| anyhow!("duration must be numeric minutes or use PT hours/minutes"))?;
    if body.is_empty() {
        bail!("duration must include hours or minutes");
    }
    let bytes = body.as_bytes();
    let mut pos = 0;
    let mut total = 0u64;
    let mut saw_hours = false;
    let mut saw_minutes = false;
    while pos < bytes.len() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if start == pos || pos == bytes.len() {
            bail!("malformed PT duration");
        }
        let amount = body[start..pos]
            .parse::<u64>()
            .context("duration component overflows u64")?;
        match bytes[pos] {
            b'H' if !saw_hours && !saw_minutes => {
                total = amount
                    .checked_mul(60)
                    .context("duration minutes overflow u64")?;
                saw_hours = true;
            }
            b'M' if !saw_minutes => {
                total = total
                    .checked_add(amount)
                    .context("duration minutes overflow u64")?;
                saw_minutes = true;
            }
            _ => bail!("duration supports ordered hours and minutes only"),
        }
        pos += 1;
    }
    if !saw_hours && !saw_minutes {
        bail!("duration must include hours or minutes");
    }
    Ok(Some(total))
}

pub(super) fn read_recipe(path: &Path) -> Result<(RecipeData, String)> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_RECIPE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECIPE_BYTES {
        bail!("file exceeds 1 MiB")
    };
    let raw = String::from_utf8(bytes).context("recipe file is not valid UTF-8")?;
    let value: toml::Value = toml::from_str(&raw)?;
    let title = value
        .get("title")
        .and_then(toml::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("required title must be a non-empty string"))?
        .to_owned();
    let mut data: RecipeData = value.try_into()?;
    data.title = title;
    Ok((data, raw))
}
