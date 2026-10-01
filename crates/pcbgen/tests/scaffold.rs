//! `pcb new`: the scaffolded board passes the BOARD.TOML gate and places every part, and the
//! command refuses an existing board. The template's circuit and layout are compiled here as
//! they are (`__NAME__` is a plain string in them). Needs KiCad's symbol libraries, not kicad-cli.

#[path = "../../../templates/board/src/circuit.rs"]
mod circuit;
#[path = "../../../templates/board/src/layout.rs"]
mod layout;

use std::collections::BTreeSet;

use pcbgen::{boardfile, scaffold};

#[test]
fn new_board() {
    let root = std::env::temp_dir().join(format!("pcbgen-scaffold-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("boards")).unwrap();
    std::os::unix::fs::symlink(pcbgen::repo_root().join("templates"), root.join("templates")).unwrap();

    let dir = scaffold::new_board(&root, "demo-1").unwrap();
    for f in ["Cargo.toml", "src/main.rs", "src/circuit.rs", "src/layout.rs", "board.toml", "spec.md", "round.md"] {
        let text = std::fs::read_to_string(dir.join(f)).unwrap();
        assert!(!text.contains("__NAME__") && !text.contains("<name>"), "{f} still has a placeholder");
    }
    assert!(std::fs::read_to_string(dir.join("Cargo.toml")).unwrap().contains("name = \"demo-1\""));
    let err = scaffold::new_board(&root, "demo-1").unwrap_err().to_string();
    assert!(err.contains("already exists"), "{err}");
    assert!(scaffold::new_board(&root, "Bad_Name").is_err());

    // the template's circuit against the generated board.toml and spec.md
    let mut c = circuit::build();
    c.name = "demo-1".into();
    let bf = boardfile::parse(&std::fs::read_to_string(dir.join("board.toml")).unwrap()).unwrap();
    let spec = boardfile::spec_ids(&std::fs::read_to_string(dir.join("spec.md")).unwrap());
    assert_eq!(boardfile::problems(&bf, &c, Some(&spec)), Vec::<String>::new());
    assert!(c.check().is_empty(), "{:?}", c.check());

    // every assembled part has an LCSC number and an MPN (the cost stage's PARTS gate)
    for p in c.parts.iter().filter(|p| p.in_bom) {
        for key in ["LCSC", "MPN"] {
            assert!(p.fields.iter().any(|(k, v)| k == key && !v.is_empty()), "{} has no {key}", p.reference);
        }
    }

    // every part placed, nothing placed that isn't a part
    let parts: BTreeSet<String> = c.parts.iter().map(|p| p.reference.clone()).collect();
    let placed: BTreeSet<String> = layout::layout().spec.places.iter().map(|(r, _)| r.clone()).collect();
    assert_eq!(parts, placed);

    std::fs::remove_dir_all(&root).unwrap();
}
