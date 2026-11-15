use crate::*;
use serde_json::json;
use std::collections::BTreeMap;

fn example() -> Document {
    let project: serde_json::Value =
        serde_json::from_str(include_str!("../../../examples/bit-nodes.json")).unwrap();
    serde_json::from_value(project["document"].clone()).unwrap()
}
#[test]
fn bit_nodes_example_evaluates_exports_and_survives_roundtrip() {
    let doc = example();
    for low in 0..16 {
        assert_eq!(
            doc.evaluate(&BTreeMap::from([("low".into(), low)]))
                .unwrap()["result"],
            0xA0 | low
        );
    }
    let rtl = doc.rtl().unwrap();
    assert!(rtl.contains("8'hAB"));
    assert!(rtl.contains("n_literal[7:4]"));
    assert!(rtl.contains("{n_upper, n_low}"));
    let loaded: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(doc, loaded);
    assert_eq!(loaded.node("upper").unwrap().inputs(), vec![("a", 8)]);
    assert_eq!(
        loaded.node("joined").unwrap().inputs(),
        vec![("hi", 4), ("lo", 4)]
    );
}
#[test]
fn every_concat_split_and_slice_boundary_is_exact_through_64_bits() {
    for width in 2..=64u8 {
        for low in 1..width {
            let hi = width - low;
            let source = mask(width);
            let doc: Document = serde_json::from_value(json!({"name":"Bits","nodes":[
                {"id":"hi","name":"hi","kind":"constant","width":hi,"x":0,"y":0,"config":{"type":"constant","value":mask(hi).to_string()}},
                {"id":"lo","name":"lo","kind":"constant","width":low,"x":0,"y":0,"config":{"type":"constant","value":mask(low).to_string()}},
                {"id":"cat","name":"cat","kind":"concat","width":width,"x":0,"y":0,"config":{"type":"concat","high_width":hi,"low_width":low}},
                {"id":"cut","name":"cut","kind":"slice","width":hi,"x":0,"y":0,"config":{"type":"slice","input_width":width,"lsb":low}},
                {"id":"result","name":"result","kind":"output","width":hi,"x":0,"y":0}
            ],"edges":[
                {"id":"e0","from":"hi","to":"cat","port":"hi"},
                {"id":"e1","from":"lo","to":"cat","port":"lo"},
                {"id":"e2","from":"cat","to":"cut","port":"a"},
                {"id":"e3","from":"cut","to":"result","port":"a"}
            ]})).unwrap();
            let values = doc.evaluate_signals(&BTreeMap::new()).unwrap();
            assert_eq!(values["cat"], source);
            assert_eq!(values["cut"], mask(hi));
        }
    }
    let mut doc = example();
    doc.nodes
        .update("literal", |n| {
            n.width = 64;
            n.config = NodeConfig::Constant {
                value: u64::MAX.to_string(),
            };
        })
        .unwrap();
    doc.nodes
        .update("upper", |n| {
            n.width = 64;
            n.config = NodeConfig::Slice {
                input_width: 64,
                lsb: 0,
            };
        })
        .unwrap();
    doc.edges.retain(|e| e.to == "upper");
    doc.nodes
        .retain(|n| n.id == "literal" || n.id == "upper" || n.id == "result");
    doc.nodes.update("result", |n| n.width = 64).unwrap();
    doc.edges.push(Edge {
        id: "out".into(),
        from: "upper".into(),
        to: "result".into(),
        port: "a".into(),
    });
    assert_eq!(doc.evaluate(&BTreeMap::new()).unwrap()["result"], u64::MAX);
}
#[test]
fn invalid_configuration_and_connections_are_atomic_including_position_fast_path() {
    let mut w = Workspace {
        document: example().into(),
        ..Workspace::default()
    };
    let original = w.snapshot();
    for (id, update) in [
        (
            "literal",
            json!({"config":{"type":"constant","value":"256"}}),
        ),
        (
            "literal",
            json!({"config":{"type":"constant","value":"18446744073709551616"}}),
        ),
        (
            "literal",
            json!({"config":{"type":"constant","value":"-1"}}),
        ),
        ("literal", json!({"config":{"type":"none"}})),
        (
            "upper",
            json!({"config":{"type":"slice","input_width":8,"lsb":5}}),
        ),
        (
            "upper",
            json!({"config":{"type":"slice","input_width":0,"lsb":0}}),
        ),
        (
            "upper",
            json!({"config":{"type":"slice","input_width":7,"lsb":0}}),
        ),
        (
            "upper",
            json!({"config":{"type":"slice","input_width":64,"lsb":255}}),
        ),
        (
            "joined",
            json!({"config":{"type":"concat","high_width":0,"low_width":8}}),
        ),
        (
            "joined",
            json!({"config":{"type":"concat","high_width":4,"low_width":5}}),
        ),
        (
            "joined",
            json!({"width":64,"config":{"type":"concat","high_width":255,"low_width":65}}),
        ),
        ("low", json!({"config":{"type":"constant","value":"0"}})),
    ] {
        let mut op = update;
        op["op"] = json!("update_node");
        op["id"] = json!(id);
        op["x"] = json!(999);
        let args = json!({"expected_revision":0,"label":"invalid","operations":[op]});
        assert!(!is_position_patch(&args));
        assert!(
            w.dispatch("design.apply_patch", args, "test").is_err(),
            "{id}"
        );
        assert_eq!(w.snapshot(), original);
    }
    // Connected output widths cannot silently change either.
    assert!(w.dispatch("design.apply_patch",json!({"expected_revision":0,"label":"invalid","operations":[{"op":"update_node","id":"low","width":3}]}),"test").is_err());
    w.dispatch("design.apply_patch",json!({"expected_revision":0,"label":"constant","operations":[{"op":"update_node","id":"literal","config":{"type":"constant","value":"0xCD"}}]}),"test").unwrap();
    assert_eq!(
        w.document
            .evaluate(&BTreeMap::from([("low".into(), 3)]))
            .unwrap()["result"],
        0xC3
    );
    w.dispatch("design.undo", json!({"expected_revision":1}), "test")
        .unwrap();
    assert_eq!(*w.document, example());
    w.dispatch("design.redo", json!({"expected_revision":2}), "test")
        .unwrap();
    assert_eq!(
        w.document
            .evaluate(&BTreeMap::from([("low".into(), 3)]))
            .unwrap()["result"],
        0xC3
    );
}
#[test]
fn bit_nodes_feed_clocked_simulation() {
    let mut doc = example();
    let reg: Node = serde_json::from_value(
        json!({"id":"delay","name":"delay","kind":"register","width":8,"x":0,"y":0}),
    )
    .unwrap();
    doc.nodes.push(reg);
    doc.edges
        .update("e3", |e| {
            e.to = "delay".into();
            e.port = "d".into();
        })
        .unwrap();
    doc.edges.push(Edge {
        id: "out".into(),
        from: "delay".into(),
        to: "result".into(),
        port: "a".into(),
    });
    let samples = doc
        .simulate_cycles(&[
            CycleInput {
                reset: true,
                inputs: BTreeMap::from([("low".into(), "3".into())]),
            },
            CycleInput {
                reset: false,
                inputs: BTreeMap::from([("low".into(), "3".into())]),
            },
        ])
        .unwrap();
    assert_eq!(samples[0].outputs["result"], "0");
    assert_eq!(samples[1].outputs["result"], "163");
}
