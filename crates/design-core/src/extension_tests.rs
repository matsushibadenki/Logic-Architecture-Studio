use crate::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn example(input: u8, output: u8) -> Document {
    let project: Value =
        serde_json::from_str(include_str!("../../../examples/extension-nodes.json")).unwrap();
    let mut doc: Document = serde_json::from_value(project["document"].clone()).unwrap();
    for n in doc.nodes.iter_mut() {
        n.width = if n.kind == Kind::Input { input } else { output };
        match &mut n.config {
            NodeConfig::ZeroExtend { input_width } | NodeConfig::SignExtend { input_width } => {
                *input_width = input
            }
            _ => {}
        }
    }
    doc
}
#[test]
fn every_extension_width_pair_matches_twos_complement_and_preserves_bits() {
    for input in 1..=64 {
        for output in input..=64 {
            let doc = example(input, output);
            let sign = 1u64 << (input - 1);
            for bits in [
                0,
                1,
                sign - 1,
                sign,
                mask(input),
                0xA55A123456789ABC & mask(input),
            ] {
                // Mathematical signed value, independent of the evaluator's bit-fill algorithm.
                let signed = i128::from(bits) - if bits >= sign { 1i128 << input } else { 0 };
                let expected = (signed & i128::from(mask(output))) as u64;
                let values = doc
                    .evaluate(&BTreeMap::from([("data".into(), bits)]))
                    .unwrap();
                assert_eq!(values["zero_out"], bits, "zero {input} → {output}");
                assert_eq!(values["sign_out"], expected, "sign {input} → {output}");
                assert_eq!(signed_value(expected, output), signed);
            }
        }
    }
    assert_eq!(signed_value(0x8000000000000000, 64), i128::from(i64::MIN));
    assert_eq!(signed_value(u64::MAX, 64), -1);
}
#[test]
fn extensions_reject_narrowing_mismatched_configs_and_connected_width_edits() {
    let mut w = Workspace {
        document: example(4, 8).into(),
        ..Workspace::default()
    };
    let before = w.snapshot();
    for update in [
        json!({"config":{"type":"sign_extend","input_width":0}}),
        json!({"config":{"type":"sign_extend","input_width":9}}),
        json!({"config":{"type":"sign_extend","input_width":65}}),
        json!({"config":{"type":"sign_extend","input_width":255}}),
        json!({"config":{"type":"sign_extend","input_width":3}}),
        json!({"config":{"type":"zero_extend","input_width":4}}),
        json!({"config":{"type":"none"}}),
        json!({"width":3}),
        json!({"width":65}),
    ] {
        let mut op = update;
        op["op"] = json!("update_node");
        op["id"] = json!("signed_bits");
        op["x"] = json!(120);
        let args = json!({"expected_revision":0,"label":"invalid extension","operations":[op]});
        assert!(!is_position_patch(&args));
        assert!(w.dispatch("design.apply_patch", args, "test").is_err());
        assert_eq!(w.snapshot(), before);
    }
    for (kind, config) in [
        (Kind::ZeroExtend, NodeConfig::ZeroExtend { input_width: 9 }),
        (Kind::SignExtend, NodeConfig::SignExtend { input_width: 9 }),
    ] {
        let mut doc = example(4, 8);
        doc.nodes
            .update("zero", |n| {
                n.kind = kind;
                n.config = config;
            })
            .unwrap();
        assert!(doc.validate(false).is_err());
    }
    w.dispatch("design.apply_patch",json!({"expected_revision":0,"label":"switch extension","operations":[{"op":"update_node","id":"zero","kind":"sign_extend","config":{"type":"sign_extend","input_width":4}}]}),"test").unwrap();
    assert_eq!(
        w.document
            .evaluate(&BTreeMap::from([("data".into(), 8)]))
            .unwrap()["zero_out"],
        248
    );
    w.dispatch("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert_eq!(*w.document, example(4, 8));
    w.dispatch("design.redo", json!({"expected_revision":2}), "test")
        .unwrap();
    let restored: Workspace = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    assert_eq!(restored.document, w.document);
}
#[test]
fn extension_rtl_is_explicit_and_equal_width_is_a_wire() {
    let rtl = example(4, 8).rtl().unwrap();
    assert!(
        rtl.contains("assign n_zero = {{4{1'b0}}, n_data};"),
        "{rtl}"
    );
    assert!(rtl.contains("assign n_signed_bits = {{4{n_data[3]}}, n_data};"));
    let rtl = example(64, 64).rtl().unwrap();
    assert!(rtl.contains("assign n_zero = n_data;"));
    assert!(rtl.contains("assign n_signed_bits = n_data;"));
    assert!(!rtl.contains("{{0{"));
}
#[test]
fn extension_bit_patterns_reach_registers_and_reset() {
    let mut doc = example(4, 8);
    let register: Node = serde_json::from_value(
        json!({"id":"delay","name":"delay","kind":"register","width":8,"x":0,"y":0}),
    )
    .unwrap();
    doc.nodes.push(register);
    doc.edges
        .update("e3", |e| {
            e.to = "delay".into();
            e.port = "d".into();
        })
        .unwrap();
    doc.edges.push(Edge {
        id: "regout".into(),
        from: "delay".into(),
        to: "sign_out".into(),
        port: "a".into(),
    });
    let cycles: Vec<_> = [
        (true, "8"),
        (false, "8"),
        (false, "15"),
        (false, "7"),
        (true, "15"),
    ]
    .into_iter()
    .map(|(reset, v)| CycleInput {
        reset,
        inputs: BTreeMap::from([("data".into(), v.into())]),
    })
    .collect();
    let samples = doc.simulate_cycles(&cycles).unwrap();
    let actual: Vec<_> = samples
        .iter()
        .map(|s| s.outputs["sign_out"].as_str())
        .collect();
    assert_eq!(actual, ["0", "248", "255", "7", "0"]);
}
