use super::*;
use serde::{
    Serialize,
    de::{MapAccess, Visitor},
};
use std::{collections::BTreeMap, fmt, marker::PhantomData};
use topcoat::router::{href, query_params};

/// Preserve the existing first-value-wins policy, including empty strings.
/// The framework still owns URL decoding and request-scoped parse caching.
#[derive(Debug)]
pub(super) struct First<T>(pub T);
impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for First<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FirstVisitor<T>(PhantomData<T>);
        impl<'de, T: serde::de::DeserializeOwned> Visitor<'de> for FirstVisitor<T> {
            type Value = First<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("URL query fields")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    fields
                        .entry(key)
                        .or_insert(serde_json::Value::String(value));
                }
                serde_json::from_value(
                    serde_json::to_value(fields).map_err(serde::de::Error::custom)?,
                )
                .map(First)
                .map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_map(FirstVisitor(PhantomData))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct ListFields {
    pub q: String,
    pub category: String,
    pub sort: String,
    limit: Option<String>,
    offset: Option<String>,
}

#[query_params(error = bad_request)]
pub(super) struct ListQuery {
    #[serde(flatten)]
    pub fields: First<ListFields>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sort {
    Alphabetical,
    Time,
}
impl Sort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alphabetical => "az",
            Self::Time => "time",
        }
    }
}

pub(super) struct ListOptions {
    pub search: String,
    pub category: String,
    pub sort: Sort,
    pub limit: usize,
    pub offset: usize,
}

impl ListQuery {
    pub fn read(cx: &Cx) -> topcoat::Result<ListOptions> {
        let fields = &query_params::<Self>(cx)?.fields.0;
        Ok(ListOptions {
            search: fields.q.clone(),
            category: fields.category.clone(),
            sort: if fields.sort == "time" {
                Sort::Time
            } else {
                Sort::Alphabetical
            },
            limit: number(fields.limit.as_deref(), "limit", 50)?.clamp(1, 100),
            offset: number(fields.offset.as_deref(), "offset", 0)?,
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct Feedback {
    pub error: Option<String>,
    pub created: Option<String>,
    pub password: Option<String>,
    pub saved: Option<String>,
    pub templates: Option<String>,
    pub synced: Option<String>,
}
#[query_params(error = bad_request)]
pub(super) struct FeedbackQuery {
    #[serde(flatten)]
    pub fields: First<Feedback>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct Selection {
    pub format: String,
    pub edit: Option<String>,
    pub from: String,
    pub recipe: Option<String>,
    pub add: Option<String>,
    pub modal: String,
}
#[query_params(error = bad_request)]
pub(super) struct SelectionQuery {
    #[serde(flatten)]
    pub fields: First<Selection>,
}

pub(super) fn number(raw: Option<&str>, name: &str, default: usize) -> topcoat::Result<usize> {
    raw.map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|_| bad_request(format!("{name} must be a non-negative integer")).into())
    })
}

#[derive(Serialize)]
struct ListLink<'a> {
    q: &'a str,
    category: &'a str,
    sort: &'a str,
    limit: usize,
    offset: usize,
}
pub(super) fn list_url(
    cx: &Cx,
    q: &str,
    category: &str,
    sort: &str,
    limit: usize,
    offset: usize,
) -> String {
    href!(super::recipes::home)
        .query(ListLink {
            q,
            category,
            sort,
            limit,
            offset,
        })
        .resolve(cx)
}

pub(super) fn recipe_url(cx: &Cx, id: i64, from: &str) -> String {
    href!(super::recipes::recipe_page, RecipeId(id))
        .query([("from", from)])
        .resolve(cx)
}

pub(super) fn list_return(cx: &Cx) -> topcoat::Result<String> {
    let from = &query_params::<SelectionQuery>(cx)?.fields.0.from;
    Ok(if from == "/" || from.starts_with("/?") {
        from.clone()
    } else {
        "/".into()
    })
}

pub(super) fn selected_recipe(cx: &Cx) -> topcoat::Result<Option<i64>> {
    let selection = &topcoat::router::query_params::<SelectionQuery>(cx)?
        .fields
        .0;
    selection
        .recipe
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|raw| {
            raw.parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(|| bad_request("invalid recipe").into())
        })
        .transpose()
}
