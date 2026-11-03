pub mod legend;
pub mod start;
pub mod statusbar;
pub mod topbar;
mod widgets;

use crate::i18n::Lang;
use snoopr_core::tree::{NodeId, NodeKind, Tree};

/// Name shown in the bars: the root path for the root, localized names for the
/// Recycle Bin and the "not scanned" block, the file name otherwise.
pub fn node_label(tree: &Tree, id: NodeId, lang: Lang) -> String {
    if id == tree.root() {
        return tree.root_path().display().to_string();
    }
    let node = tree.node(id);
    if node.kind == NodeKind::Unscanned {
        lang.s().unscanned_name.to_string()
    } else if tree.recycle_bin() == Some(id) {
        lang.s().recycle_bin_name.to_string()
    } else {
        node.name.to_string_lossy().into_owned()
    }
}
