use super::*;
use crate::testbench::Testbench;
#[test]
fn typed_decimal_and_hex_boundaries_for_all_widths() {
    for width in 1..=64 {
        let bound = 1i128 << (width - 1);
        for value in [-bound, -1, 0, bound - 1] {
            let bits = parse_typed_value(&value.to_string(), width, true).unwrap();
            assert_eq!(signed_value(bits, width), value);
            assert_eq!(
                parse_typed_value(&format!("0x{bits:X}"), width, true).unwrap(),
                bits
            );
        }
        for value in [-bound - 1, bound] {
            assert!(parse_typed_value(&value.to_string(), width, true).is_err());
        }
        assert!(parse_typed_value("-1", width, false).is_err());
        assert_eq!(
            parse_typed_value(&mask(width).to_string(), width, false).unwrap(),
            mask(width)
        );
        if width < 64 {
            assert!(parse_typed_value(&format!("0x{:X}", 1u64 << width), width, true).is_err());
        }
    }
}
#[test]
fn signed_propagation_is_atomic_and_preserves_selectors_history_and_legacy() {
    let mut w = Workspace::default();
    assert!(!serde_json::to_string(&w.document)
        .unwrap()
        .contains("signed"));
    w.dispatch(
        "design.set_signed",
        json!({"id":"a","signed":true,"expected_revision":0}),
        "test",
    )
    .unwrap();
    for n in &w.document.nodes {
        assert_eq!(n.signed, n.id != "op");
    }
    assert!(w
        .dispatch(
            "design.set_signed",
            json!({"id":"op","signed":true,"expected_revision":1}),
            "test"
        )
        .is_err());
    assert_eq!(w.revision, 1);
    assert!(w.dispatch("design.apply_patch",json!({"expected_revision":1,"label":"mixed","operations":[{"op":"update_node","id":"a","signed":false,"x":100}]}),"test").is_err());
    w.dispatch("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert!(w.document.nodes.iter().all(|n| !n.signed));
    w.dispatch("design.redo", json!({"expected_revision":2}), "test")
        .unwrap();
    let restored: Workspace = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    assert_eq!(w.document, restored.document);
    assert!(w
        .document
        .rtl()
        .unwrap()
        .contains("input wire signed [31:0] n_a"));
    assert!(w.document.rtl().unwrap().contains("input wire [1:0] n_op"));
}
#[test]
fn signed_alu_wraps_and_checks_negative_expectations() {
    let mut doc = Document::demo().set_signed("result", true).unwrap();
    for n in doc.nodes.iter_mut() {
        if n.id != "op" {
            n.width = 8;
        }
    }
    let csv="cycle,reset,in.a,in.b,in.op,expect.result\n1,0,-128,-1,0,127\n2,0,127,1,0,-128\n3,0,-1,-128,1,-128\n4,0,-1,-128,2,127\n5,0,0xFF,0,0,-1\n";
    let rows = Testbench::import_csv(&doc, csv).unwrap();
    let table = Testbench { version: 1, rows };
    let (_, samples, report) = table.run(&doc).unwrap();
    assert_eq!(report.passed, 5);
    assert_eq!(samples[1].outputs["result"], "-128");
    assert_eq!(
        Testbench::import_csv(&doc, &table.export_csv(&doc).unwrap()).unwrap(),
        table.rows
    );
    assert!(Testbench::import_csv(&doc, &csv.replace("127,1", "128,1")).is_err());
    assert!(Testbench::import_csv(&doc, &csv.replace(",0,-1\n", ",0,128\n")).is_err());
}
#[test]
fn extension_boundaries_and_signed_constant_slice_concat_register() {
    let project: Value =
        serde_json::from_str(include_str!("../../../examples/extension-nodes.json")).unwrap();
    let doc: Document = serde_json::from_value(project["document"].clone()).unwrap();
    let doc = doc.set_signed("signed_bits", true).unwrap();
    assert!(!doc.node("data").unwrap().signed);
    assert!(doc.node("sign_out").unwrap().signed);
    let doc = doc.set_signed("data", true).unwrap();
    assert!(!doc.node("zero").unwrap().signed);
    let input = doc
        .parse_signals(&BTreeMap::from([("data".into(), "-8".into())]), Kind::Input)
        .unwrap();
    assert_eq!(
        doc.format_outputs(&doc.evaluate(&input).unwrap())["sign_out"],
        "-8"
    );
    let project: Value =
        serde_json::from_str(include_str!("../../../examples/bit-nodes.json")).unwrap();
    let mut doc: Document = serde_json::from_value(project["document"].clone()).unwrap();
    // Hex constants preserve their bit patterns while changing the type.
    for n in doc.nodes.iter_mut() {
        if let NodeConfig::Constant { value } = &n.config {
            let bits = number(value).unwrap();
            n.config = NodeConfig::Constant {
                value: format!("0x{bits:X}"),
            };
        }
        n.signed = true;
    }
    doc.validate(true).unwrap();
    let n = doc
        .nodes
        .iter_mut()
        .find(|n| n.kind == Kind::Constant)
        .unwrap();
    n.config = NodeConfig::Constant { value: "-1".into() };
    assert_eq!(n.parse_value("-1").unwrap(), mask(n.width));
    doc.validate(true).unwrap();
    assert!(doc.rtl().unwrap().contains("wire signed"));
}

#[test]
fn incompatible_constant_type_change_does_not_mutate_document() {
    let mut doc = Document::demo();
    doc.nodes.push(Node {
        id: "limit".into(),
        name: "limit".into(),
        kind: Kind::Constant,
        width: 8,
        signed: false,
        config: NodeConfig::Constant {
            value: "255".into(),
        },
        x: 0.,
        y: 0.,
    });
    assert!(doc.set_signed("limit", true).is_err());
    assert!(!doc.node("limit").unwrap().signed);
    doc.nodes
        .update("limit", |n| {
            n.config = NodeConfig::Constant {
                value: "0xFF".into(),
            }
        })
        .unwrap();
    let typed = doc.set_signed("limit", true).unwrap();
    assert_eq!(typed.node("limit").unwrap().decimal(255), "-1");
}
