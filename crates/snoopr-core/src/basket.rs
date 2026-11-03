use crate::tree::{NodeId, Tree};

#[derive(Default, Debug, Clone)]
pub struct Basket {
    items: Vec<NodeId>,
}

impl Basket {
    pub fn items(&self) -> &[NodeId] {
        &self.items
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn contains(&self, id: NodeId) -> bool {
        self.items.contains(&id)
    }
    /// Strict ancestor of `id` present in the basket.
    pub fn covering(&self, tree: &Tree, id: NodeId) -> Option<NodeId> {
        self.items
            .iter()
            .copied()
            .find(|&it| tree.is_ancestor(it, id))
    }
    /// No effect if present or covered; removes the descendants of `id`.
    pub fn add(&mut self, tree: &Tree, id: NodeId) {
        if self.contains(id) || self.covering(tree, id).is_some() {
            return;
        }
        self.items.retain(|&it| !tree.is_ancestor(id, it));
        self.items.push(id);
    }
    pub fn remove(&mut self, id: NodeId) {
        self.items.retain(|&it| it != id);
    }
    pub fn total_size(&self, tree: &Tree) -> u64 {
        self.items.iter().map(|&it| tree.node(it).size).sum()
    }
    pub fn retain(&mut self, mut f: impl FnMut(NodeId) -> bool) {
        self.items.retain(|&it| f(it));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{EntryData, NodeKind};

    fn e(name: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: name.into(),
            kind,
            size,
            newest: 0,
        }
    }

    fn setup() -> (Tree, NodeId, NodeId) {
        let mut t = Tree::new(r"C:\".into(), 0);
        let r = t.root();
        let users = t.add_children(r, vec![e("Users", NodeKind::Dir, 0)])[0];
        let lenny = t.add_children(users, vec![e("lenny", NodeKind::Dir, 0)])[0];
        let videos = t.add_children(lenny, vec![e("Videos", NodeKind::Dir, 0)])[0];
        let v = t.add_children(videos, vec![e("v.mp4", NodeKind::File, 100)])[0];
        let w = t.add_children(r, vec![e("w.bin", NodeKind::File, 50)])[0];
        let _ = w;
        (t, videos, v)
    }

    #[test]
    fn add_folder_removes_descendants() {
        let (t, videos, v) = setup();
        let mut b = Basket::default();
        b.add(&t, v);
        b.add(&t, videos);
        assert_eq!(b.items(), &[videos]);
    }

    #[test]
    fn covered_item_cannot_be_added() {
        let (t, videos, v) = setup();
        let mut b = Basket::default();
        b.add(&t, videos);
        b.add(&t, v);
        assert_eq!(b.items(), &[videos]);
        assert_eq!(b.covering(&t, v), Some(videos));
    }

    #[test]
    fn total_size_sums_items() {
        let (t, _videos, v) = setup();
        let w = t.child_by_name(t.root(), "w.bin".as_ref()).unwrap();
        let mut b = Basket::default();
        b.add(&t, v);
        b.add(&t, w);
        assert_eq!(b.total_size(&t), 150);
    }
}
