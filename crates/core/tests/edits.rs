//! Editing semantics that the apps rely on: what undo takes back, what nests how deep.

use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec, Side};

#[test]
fn taking_back_a_delete_takes_back_only_the_delete() {
    // The page is deleted, the snackbar offers "Rückgängig"; meanwhile another page is renamed
    // (or marked, or the paper changed). Undo brings the page back and keeps the rename.
    let nb = Notebook::create(1, "x").unwrap();
    let (a, sheet_a) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
    let (b, _) = nb.add_sheet(None, 1, &SheetSpec::a4(Background::Grid)).unwrap();
    let (_, side_a) = nb.add_side_sheet(&a, Side::Right, 0, &SheetSpec::a4(Background::Blank)).unwrap();
    nb.delete_undoable(&a).unwrap();
    nb.set_title_of(&b, "Umbenannt").unwrap();
    nb.set_marked(&b, true).unwrap();
    assert!(nb.undo_delete().unwrap());
    let rows = nb.outline(true);
    assert_eq!(rows.len(), 2, "the page is back: {rows:?}");
    let title_b = rows.iter().find_map(|r| match r {
        OutlineRow::Sheet { node, title, .. } if *node == b => Some(title.clone()),
        _ => None,
    });
    assert_eq!(title_b.as_deref(), Some("Umbenannt"), "the rename stays");
    assert_eq!(nb.marked(), vec![b.clone()]);
    // In its place, with its side sheet; the same documents (the outline node may be new).
    assert!(matches!(&rows[0], OutlineRow::Sheet { sheet_id, right, .. } if *sheet_id == sheet_a && right.len() == 1 && right[0].sheet_id == side_a), "{rows:?}");
}

#[test]
fn a_tree_nested_absurdly_deep_does_not_overflow_the_stack() {
    // Groups nested hundreds deep (a hostile or broken peer): the outline stops at a sane
    // depth instead of recursing until the stack runs out (on a small thread, like the app's).
    let nb = Notebook::create(1, "tief").unwrap();
    let mut parent: Option<String> = None;
    for _ in 0..800 {
        parent = Some(nb.add_group(parent.as_deref(), 0, "g").unwrap());
    }
    nb.add_sheet(parent.as_deref(), 0, &SheetSpec::a4(Background::Blank)).unwrap();
    let bytes = nb.snapshot().unwrap();
    std::thread::Builder::new()
        .stack_size(64 << 10)
        .spawn(move || {
            let nb = Notebook::load(2, &bytes).unwrap();
            let rows = nb.outline(true);
            assert!(!rows.is_empty() && rows.len() < 801);
            let _ = nb.sheet_order();
            let _ = nb.sheet_specs();
            let _ = nb.marked();
        })
        .unwrap()
        .join()
        .expect("no stack overflow");
}
