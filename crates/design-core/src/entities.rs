//! Ordered, JSON-array-compatible copy-on-write entity blocks.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::BTreeMap,
    ops::{Index, IndexMut},
    sync::{Arc, OnceLock},
};
pub trait Entity {
    fn id(&self) -> &str;
}
const BLOCK: usize = 32;
#[derive(Debug, Clone)]
pub struct Entities<T> {
    blocks: Arc<Vec<Arc<Vec<T>>>>,
    index: Arc<OnceLock<BTreeMap<String, usize>>>,
    len: usize,
    bytes: Arc<Vec<Arc<OnceLock<usize>>>>,
}
impl<T: Entity + Clone> From<Vec<T>> for Entities<T> {
    fn from(values: Vec<T>) -> Self {
        let index: BTreeMap<String, usize> = values
            .iter()
            .enumerate()
            .map(|(i, v)| (v.id().to_owned(), i))
            .collect();
        let len = values.len();
        Self {
            blocks: Arc::new(values.chunks(BLOCK).map(|c| Arc::new(c.to_vec())).collect()),
            index: Arc::new(OnceLock::from(index)),
            bytes: Arc::new(
                (0..len.div_ceil(BLOCK))
                    .map(|_| Arc::new(OnceLock::new()))
                    .collect(),
            ),
            len,
        }
    }
}
impl<T: Entity + Clone> FromIterator<T> for Entities<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Vec::from_iter(iter).into()
    }
}
impl<T> Entities<T> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.blocks.iter().flat_map(|b| b.iter())
    }
    pub fn same_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.blocks, &other.blocks)
    }
}
impl<T: Entity + Clone> Entities<T> {
    fn positions(&self) -> &BTreeMap<String, usize> {
        self.index.get_or_init(|| {
            self.iter()
                .enumerate()
                .map(|(i, v)| (v.id().to_owned(), i))
                .collect()
        })
    }
    pub fn by_id(&self, id: &str) -> Option<&T> {
        self.positions().get(id).map(|i| &self[*i])
    }
    pub fn update(&mut self, id: &str, edit: impl FnOnce(&mut T)) -> Result<(), String> {
        let i = *self.positions().get(id).ok_or("Unknown entity")?;
        Arc::make_mut(&mut self.bytes)[i / BLOCK] = Arc::new(OnceLock::new());
        let blocks = Arc::make_mut(&mut self.blocks);
        let value = &mut Arc::make_mut(&mut blocks[i / BLOCK])[i % BLOCK];
        edit(value);
        if value.id() != id {
            self.index = Arc::new(OnceLock::new());
        }
        Ok(())
    }
    /// Only shared-order blocks can be compared sparsely. Structural changes
    /// return None and use the general identity/reordering path.
    pub fn changed<'a>(&'a self, old: &'a Self) -> Option<Vec<(&'a T, &'a T)>>
    where
        T: PartialEq,
    {
        if !Arc::ptr_eq(&self.index, &old.index) || self.len != old.len {
            return None;
        }
        Some(
            self.blocks
                .iter()
                .zip(old.blocks.iter())
                .filter(|(a, b)| !Arc::ptr_eq(a, b))
                .flat_map(|(a, b)| a.iter().zip(b.iter()))
                .filter(|(a, b)| a != b)
                .collect(),
        )
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.index = Arc::new(OnceLock::new());
        self.bytes = Arc::new(
            (0..self.blocks.len())
                .map(|_| Arc::new(OnceLock::new()))
                .collect(),
        );
        Arc::make_mut(&mut self.blocks)
            .iter_mut()
            .flat_map(|b| Arc::make_mut(b).iter_mut())
    }
    pub fn to_vec(&self) -> Vec<T> {
        self.iter().cloned().collect()
    }
    pub fn push(&mut self, v: T) {
        let mut all = self.to_vec();
        all.push(v);
        *self = all.into();
    }
    pub fn pop(&mut self) -> Option<T> {
        let mut all = self.to_vec();
        let v = all.pop();
        *self = all.into();
        v
    }
    pub fn retain(&mut self, f: impl FnMut(&T) -> bool) {
        let mut all = self.to_vec();
        all.retain(f);
        *self = all.into();
    }
    pub fn reverse(&mut self) {
        let mut all = self.to_vec();
        all.reverse();
        *self = all.into();
    }
}
impl<T> Index<usize> for Entities<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        assert!(i < self.len);
        &self.blocks[i / BLOCK][i % BLOCK]
    }
}
impl<T: Entity + Clone> IndexMut<usize> for Entities<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        assert!(i < self.len);
        self.index = Arc::new(OnceLock::new());
        Arc::make_mut(&mut self.bytes)[i / BLOCK] = Arc::new(OnceLock::new());
        &mut Arc::make_mut(&mut Arc::make_mut(&mut self.blocks)[i / BLOCK])[i % BLOCK]
    }
}
impl<'a, T> IntoIterator for &'a Entities<T> {
    type Item = &'a T;
    type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}
impl<T: PartialEq> PartialEq for Entities<T> {
    fn eq(&self, other: &Self) -> bool {
        self.same_storage(other)
            || (self.len == other.len
                && self
                    .blocks
                    .iter()
                    .zip(other.blocks.iter())
                    .all(|(a, b)| Arc::ptr_eq(a, b) || a == b))
    }
}
impl<T: Serialize> Serialize for Entities<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}
impl<'de, T: Deserialize<'de> + Entity + Clone> Deserialize<'de> for Entities<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<T>::deserialize(d).map(Into::into)
    }
}

impl<'a, T: Entity + Clone> IntoIterator for &'a mut Entities<T> {
    type Item = &'a mut T;
    type IntoIter = Box<dyn Iterator<Item = &'a mut T> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter_mut())
    }
}

impl<T: Serialize> Entities<T> {
    pub fn encoded_len(&self) -> usize {
        2 + self
            .blocks
            .iter()
            .zip(self.bytes.iter())
            .map(|(block, cache)| {
                cache
                    .get_or_init(|| crate::encoded_size(block))
                    .saturating_sub(2)
            })
            .sum::<usize>()
            + self.blocks.len().saturating_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Kind, Node};
    #[test]
    fn blocks_share_untouched_entities_and_keep_json_sizes_exact() {
        let nodes: Entities<Node> = (0..2000)
            .map(|i| Node {
                id: format!("n{i}"),
                name: format!("n{i}"),
                kind: Kind::Input,
                width: 32,
                x: i as f32,
                y: 0.,
            })
            .collect();
        let mut doc = Document {
            name: "Large".into(),
            nodes,
            edges: Vec::new().into(),
        };
        assert_eq!(doc.encoded_len(), serde_json::to_vec(&doc).unwrap().len());
        let old = doc.clone();
        doc.nodes.update("n1000", |n| n.x = 12345.5).unwrap();
        assert_eq!(doc.nodes.changed(&old.nodes).unwrap().len(), 1);
        assert_eq!(
            doc.nodes
                .blocks
                .iter()
                .zip(old.nodes.blocks.iter())
                .filter(|(a, b)| !Arc::ptr_eq(a, b))
                .count(),
            1
        );
        assert!(doc.edges.same_storage(&old.edges));
        assert_eq!(old.node("n1000").unwrap().x, 1000.);
        assert_eq!(doc.encoded_len(), serde_json::to_vec(&doc).unwrap().len());
        doc.nodes[1000].id = "renamed".into();
        assert!(doc.nodes.by_id("n1000").is_none());
        assert!(doc.nodes.by_id("renamed").is_some());
        assert!(doc.nodes.changed(&old.nodes).is_none());
        for n in doc.nodes.iter_mut().take(2) {
            n.x = -12.;
        }
        doc.nodes.reverse();
        doc.nodes.pop();
        doc.nodes.retain(|n| n.id != "n42");
        assert_eq!(doc.encoded_len(), serde_json::to_vec(&doc).unwrap().len());
        let decoded: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(doc, decoded);
        assert_eq!(decoded.encoded_len(), doc.encoded_len());
    }
}
