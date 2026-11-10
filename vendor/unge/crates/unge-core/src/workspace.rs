use crate::{Document, Editor, Error, Id, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Locale {
    #[default]
    En,
    Ja,
    ZhCn,
}
/// Only references are serialized. Resource bytes/textures stay in host-owned stores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetReference {
    pub uri: String,
    pub media_type: String,
    pub content_version: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceFile {
    pub schema_version: u32,
    pub documents: Vec<Document>,
    #[serde(default)]
    pub assets: BTreeMap<Id, AssetReference>,
    #[serde(default)]
    pub locale: Locale,
}
/// A Rust application can retain many documents, each with its own history.
pub struct Workspace {
    editors: BTreeMap<Id, Editor>,
    pub assets: BTreeMap<Id, AssetReference>,
    pub locale: Locale,
}
impl Workspace {
    pub fn from_file(file: WorkspaceFile, history_limit: usize) -> Result<Self> {
        if file.schema_version != 1 {
            return Err(Error::Version(file.schema_version));
        }
        let mut workspace = Self {
            editors: BTreeMap::new(),
            assets: file.assets,
            locale: file.locale,
        };
        for document in file.documents {
            workspace.insert(document, history_limit)?;
        }
        Ok(workspace)
    }
    pub fn insert(&mut self, document: Document, history_limit: usize) -> Result<Id> {
        let id = document.graph().id;
        if self.editors.contains_key(&id) {
            return Err(Error::Duplicate(id.to_string()));
        }
        self.editors
            .insert(id, Editor::new(document, history_limit)?);
        Ok(id)
    }
    pub fn editor(&self, id: Id) -> Option<&Editor> {
        self.editors.get(&id)
    }
    pub fn editor_mut(&mut self, id: Id) -> Option<&mut Editor> {
        self.editors.get_mut(&id)
    }
    pub fn to_file(&self) -> WorkspaceFile {
        WorkspaceFile {
            schema_version: 1,
            documents: self
                .editors
                .values()
                .map(|e| e.document().clone())
                .collect(),
            assets: self.assets.clone(),
            locale: self.locale,
        }
    }
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            editors: BTreeMap::new(),
            assets: BTreeMap::new(),
            locale: Locale::En,
        }
    }
}
