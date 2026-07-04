use std::io::Write;

fn main() {
    let root = std::env::args()
        .nth(1)
        .expect("usage: dted_tree_to_mmap_store <dted-root>");
    let bytes = sidereon_core::terrain_store::dted_tree_to_mmap_store(root)
        .expect("convert DTED tree to terrain store");
    std::io::stdout()
        .write_all(&bytes)
        .expect("write terrain store bytes");
}
