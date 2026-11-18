//! Random concurrent edits on three replicas, synced in random pairs, must converge.

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec, Side};
use n0te_core::sheet::Sheet;

/// Small deterministic PRNG (xorshift), so failures reproduce from the seed.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn shape(nb: &Notebook) -> Vec<String> {
    nb.outline(true)
        .into_iter()
        .map(|r| match r {
            OutlineRow::Group { node, depth, title, collapsed, .. } => format!("G{depth}:{node}:{title}:{collapsed}"),
            OutlineRow::Sheet { node, depth, left, right, .. } => format!("S{depth}:{node}:{}|{}", left.len(), right.len()),
        })
        .collect()
}

fn sync(a: &Notebook, b: &Notebook) {
    let (va, vb) = (a.version(), b.version());
    a.import(&b.updates_since(&va).unwrap()).unwrap();
    b.import(&a.updates_since(&vb).unwrap()).unwrap();
}

#[test]
fn notebooks_converge_under_random_concurrent_edits() {
    for seed in 1..=20u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let first = Notebook::create(1, "t").unwrap();
        let snap = first.snapshot().unwrap();
        let reps = [first, Notebook::load(2, &snap).unwrap(), Notebook::load(3, &snap).unwrap()];
        for _ in 0..60 {
            let r = &reps[rng.below(3)];
            let rows = r.outline(true);
            let groups: Vec<String> = rows.iter().filter_map(|x| if let OutlineRow::Group { node, .. } = x { Some(node.clone()) } else { None }).collect();
            let sheets: Vec<String> = rows.iter().filter_map(|x| if let OutlineRow::Sheet { node, .. } = x { Some(node.clone()) } else { None }).collect();
            match rng.below(7) {
                0 => {
                    r.add_group(None, rng.below(5), "g").unwrap();
                }
                1 | 2 => {
                    let parent = if !groups.is_empty() && rng.below(2) == 0 { Some(groups[rng.below(groups.len())].as_str()) } else { None };
                    r.add_sheet(parent, rng.below(5), &SheetSpec::a4(Background::Lines)).unwrap();
                }
                3 if !sheets.is_empty() => {
                    let side = if rng.below(2) == 0 { Side::Left } else { Side::Right };
                    r.add_side_sheet(&sheets[rng.below(sheets.len())], side, rng.below(3), &SheetSpec::board()).unwrap();
                }
                4 if !sheets.is_empty() => {
                    let parent = if !groups.is_empty() && rng.below(2) == 0 { Some(groups[rng.below(groups.len())].clone()) } else { None };
                    let _ = r.move_node(&sheets[rng.below(sheets.len())], parent.as_deref(), rng.below(4));
                }
                5 if !groups.is_empty() => {
                    let _ = r.set_collapsed(&groups[rng.below(groups.len())], rng.below(2) == 0);
                }
                6 if !sheets.is_empty() && rng.below(3) == 0 => {
                    r.delete(&sheets[rng.below(sheets.len())]).unwrap();
                }
                _ => {}
            }
            if rng.below(4) == 0 {
                let (i, j) = (rng.below(3), rng.below(3));
                if i != j {
                    sync(&reps[i], &reps[j]);
                }
            }
        }
        sync(&reps[0], &reps[1]);
        sync(&reps[1], &reps[2]);
        sync(&reps[0], &reps[1]);
        let (s0, s1, s2) = (shape(&reps[0]), shape(&reps[1]), shape(&reps[2]));
        assert_eq!(s0, s1, "seed {seed}");
        assert_eq!(s1, s2, "seed {seed}");
    }
}

#[test]
fn sheets_converge_under_random_strokes_and_erases() {
    for seed in 1..=20u64 {
        let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03));
        let a = Sheet::create(1).unwrap();
        let snap = a.snapshot().unwrap();
        let reps = [a, Sheet::load(2, &snap).unwrap(), Sheet::load(3, &snap).unwrap()];
        for step in 0..80 {
            let r = &reps[rng.below(3)];
            let ids: Vec<String> = r.strokes().into_iter().map(|s| s.id).collect();
            match rng.below(4) {
                0 | 1 => {
                    let n = 1 + rng.below(30);
                    let x = rng.below(500) as f32;
                    let pts: Vec<InkPoint> = (0..n).map(|i| InkPoint { x: x + i as f32, y: step as f32, pressure: 0.5, t_ms: i as u32 }).collect();
                    r.add_stroke(Brush::default(), &pts).unwrap();
                }
                2 if !ids.is_empty() => r.remove(&[ids[rng.below(ids.len())].clone()]).unwrap(),
                3 if !ids.is_empty() => r.recolor(&[ids[rng.below(ids.len())].clone()], rng.next() as u32).unwrap(),
                _ => {}
            }
            if rng.below(3) == 0 {
                let (i, j) = (rng.below(3), rng.below(3));
                if i != j {
                    let (vi, vj) = (reps[i].version(), reps[j].version());
                    reps[i].import(&reps[j].updates_since(&vi).unwrap()).unwrap();
                    reps[j].import(&reps[i].updates_since(&vj).unwrap()).unwrap();
                }
            }
        }
        for (i, j) in [(0, 1), (1, 2), (0, 1)] {
            let (vi, vj) = (reps[i].version(), reps[j].version());
            reps[i].import(&reps[j].updates_since(&vi).unwrap()).unwrap();
            reps[j].import(&reps[i].updates_since(&vj).unwrap()).unwrap();
        }
        let key = |s: &Sheet| {
            let mut v: Vec<(String, u32)> = s.strokes().into_iter().map(|s| (s.id, s.brush.color)).collect();
            v.sort();
            v
        };
        assert_eq!(key(&reps[0]), key(&reps[1]), "seed {seed}");
        assert_eq!(key(&reps[1]), key(&reps[2]), "seed {seed}");
    }
}
