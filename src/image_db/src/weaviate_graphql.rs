#[allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::db::Id;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct WeaviateInput {
    class: String,
    vector: Option<Vec<f32>>,
    properties: HashMap<String, String>,
    id: Option<Id>,
}

impl WeaviateInput {
    pub(crate) fn class(class: String) -> Self {
        WeaviateInput {
            class,
            vector: None,
            properties: HashMap::new(),
            id: None,
        }
    }

    pub(crate) fn vector(mut self, vector: Vec<f32>) -> Self {
        self.vector = Some(vector);
        self
    }

    pub(crate) fn property(mut self, key: String, value: String) -> Self {
        self.properties.insert(key, value);
        self
    }

    pub(crate) fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }
}

#[derive(Serialize)]
pub struct WeaviateBatchInput {
    objects: Vec<WeaviateInput>,
}

impl WeaviateBatchInput {
    pub(crate) fn new(objects: Vec<WeaviateInput>) -> Self {
        WeaviateBatchInput { objects }
    }
}

#[derive(Serialize)]
pub enum MultiOperator {
    And,
    Or,
}

#[derive(Serialize)]
pub enum Operator {
    And,
    Or,
    Not,
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanEqual,
    LessThan,
    LessThanEqual,
    Like,
    WithinGeoRange,
}

#[derive(Serialize)]
pub enum WhereValue {
    #[serde(rename = "valueInt")]
    Int(i64),
    #[serde(rename = "valueBoolean")]
    Boolean(bool),
    #[serde(rename = "valueString")]
    String(String),
    #[serde(rename = "valueText")]
    Text(String),
    #[serde(rename = "valueNumber")]
    Number(f64),
}

/// where { operator: Or { operands: [ {path: ["id"], operator: "Equal", valueString: id }, .. ] } }

#[derive(Serialize)]
#[serde(untagged)]
pub enum WeaviateWhere {
    Single {
        path: Vec<String>,
        operator: Operator,
        #[serde(flatten)]
        value: WhereValue,
    },
    Multiple {
        operator: MultiOperator,
        operands: Vec<WeaviateWhere>,
    },
}

#[derive(Serialize)]
pub struct WeaviateMatch {
    pub(crate) class: String,
    #[serde(rename = "where")]
    pub(crate) where_: WeaviateWhere,
}

#[derive(Serialize)]
pub enum Output {
    #[serde(rename = "minimal")]
    Minimal,
    #[serde(rename = "verbose")]
    Verbose,
}

#[derive(Serialize)]
pub struct WeaviateBatchDelete {
    #[serde(rename = "match")]
    match_: WeaviateMatch,
    output: Option<Output>,
    #[serde(rename = "dryRun")]
    dry_run: Option<bool>,
}

impl WeaviateBatchDelete {
    pub fn new(match_: WeaviateMatch) -> Self {
        Self {
            match_,
            output: None,
            dry_run: None,
        }
    }
}

#[derive(Serialize)]
pub struct VectorizerInput<'a> {
    pub texts: Vec<String>,
    pub images: Vec<Cow<'a, str>>,
}

#[derive(Deserialize)]
pub struct VectorizerOutput {
    #[serde(rename = "textVectors")]
    pub text_vectors: Vec<Vec<f32>>,
    #[serde(rename = "imageVectors")]
    pub image_vectors: Vec<Vec<f32>>,
}

#[derive(Deserialize, Debug)]
pub struct QueryResult {
    pub data: Get,
}

#[derive(Deserialize, Debug)]
pub struct Get {
    #[serde(rename = "Get")]
    pub get: HashMap<String, Vec<QueryOutput>>,
}

#[derive(Deserialize, Debug)]
pub struct QueryOutput {
    #[serde(rename = "_additional")]
    pub additional: Option<HashMap<String, Value>>,
}
