use crate::basket::Basket;
use crate::rules::{Block, RuleContext, Verdict, classify};
use crate::tree::{NodeId, NodeKind, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disabled {
    Root,
    Unscanned,
    Blocked(Block),
    CoveredBy(NodeId),
    InRecycleBin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryAction {
    Add,
    Remove,
    EmptyRecycleBin,
    Disabled(Disabled),
}

pub fn verdict(tree: &Tree, ctx: &RuleContext, id: NodeId) -> Verdict {
    classify(&tree.path(id), ctx)
}

pub fn primary_action(
    tree: &Tree,
    basket: &Basket,
    ctx: &RuleContext,
    id: NodeId,
) -> PrimaryAction {
    if id == tree.root() {
        return PrimaryAction::Disabled(Disabled::Root);
    }
    if let Some(bin) = tree.recycle_bin() {
        if id == bin {
            return PrimaryAction::EmptyRecycleBin;
        }
        if tree.is_ancestor(bin, id) {
            return PrimaryAction::Disabled(Disabled::InRecycleBin);
        }
    }
    if tree.node(id).kind == NodeKind::Unscanned {
        return PrimaryAction::Disabled(Disabled::Unscanned);
    }
    if let Verdict::Block(b) = verdict(tree, ctx, id) {
        return PrimaryAction::Disabled(Disabled::Blocked(b));
    }
    if let Some(c) = basket.covering(tree, id) {
        return PrimaryAction::Disabled(Disabled::CoveredBy(c));
    }
    if basket.contains(id) {
        return PrimaryAction::Remove;
    }
    PrimaryAction::Add
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::EntryData;

    fn ctx() -> RuleContext {
        RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: vec![
                r"C:\Users\lenny\OneDrive".into(),
                r"C:\Users\lenny\Dropbox".into(),
            ],
        }
    }

    fn e(name: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: name.into(),
            kind,
            size,
            newest: 0,
        }
    }

    #[test]
    fn primary_action_table() {
        use Disabled as D;
        use PrimaryAction as P;
        let mut t = Tree::new(r"C:\".into(), 0);
        let r = t.root();
        let top = t.add_children(
            r,
            vec![
                e("Users", NodeKind::Dir, 0),
                e("Windows", NodeKind::Dir, 0),
                e("$Recycle.Bin", NodeKind::Dir, 0),
            ],
        );
        let (users, windows, bin) = (top[0], top[1], top[2]);
        let lenny = t.add_children(users, vec![e("lenny", NodeKind::Dir, 0)])[0];
        let videos = t.add_children(lenny, vec![e("Videos", NodeKind::Dir, 0)])[0];
        let v = t.add_children(videos, vec![e("v.mp4", NodeKind::File, 100)])[0];
        t.add_children(windows, vec![e("w.dll", NodeKind::File, 10)]);
        let old = t.add_children(bin, vec![e("old.zip", NodeKind::File, 5)])[0];
        t.set_unscanned(10);
        let unscanned = t
            .node(r)
            .children
            .iter()
            .copied()
            .find(|&c| t.node(c).kind == NodeKind::Unscanned)
            .unwrap();

        let c = ctx();
        let mut b = Basket::default();
        assert_eq!(primary_action(&t, &b, &c, r), P::Disabled(D::Root));
        assert_eq!(primary_action(&t, &b, &c, bin), P::EmptyRecycleBin);
        assert_eq!(
            primary_action(&t, &b, &c, old),
            P::Disabled(D::InRecycleBin)
        );
        assert_eq!(
            primary_action(&t, &b, &c, unscanned),
            P::Disabled(D::Unscanned)
        );
        assert_eq!(
            primary_action(&t, &b, &c, windows),
            P::Disabled(D::Blocked(Block::WindowsDir))
        );
        assert_eq!(
            primary_action(&t, &b, &c, lenny),
            P::Disabled(D::Blocked(Block::ProfileRoot))
        );
        assert_eq!(primary_action(&t, &b, &c, videos), P::Add);
        b.add(&t, videos);
        assert_eq!(
            primary_action(&t, &b, &c, v),
            P::Disabled(D::CoveredBy(videos))
        );
        assert_eq!(primary_action(&t, &b, &c, videos), P::Remove);
    }

    /// Outside a drive root, `$Recycle.Bin` is an ordinary folder: never
    /// "Empty Recycle Bin", which would empty the drive's real bin.
    #[test]
    fn recycle_bin_name_below_a_folder_root_is_ordinary() {
        use PrimaryAction as P;
        let mut t = Tree::new(r"D:\Backup".into(), 0);
        let r = t.root();
        let bin = t.add_children(r, vec![e("$Recycle.Bin", NodeKind::Dir, 0)])[0];
        let old = t.add_children(bin, vec![e("old.zip", NodeKind::File, 5)])[0];
        let (b, c) = (Basket::default(), ctx());
        assert_ne!(primary_action(&t, &b, &c, bin), P::EmptyRecycleBin);
        assert_eq!(primary_action(&t, &b, &c, bin), P::Add);
        assert_eq!(primary_action(&t, &b, &c, old), P::Add);
    }
}
