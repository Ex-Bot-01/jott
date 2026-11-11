use jott_core::np::handshake::{parse_initialize, MIN_NP};
use serde_json::json;

/// The handshake line captured verbatim from np 0.3.1 (tests/fixtures).
fn real() -> serde_json::Value {
    json!({"data_dir":"/home/agent/.local/share",
           "features":{"fetch":true,"local_model":true,"vision":false},
           "methods":["status.get","initialize","search","scan","embed","models.list","shutdown"],
           "np":"0.3.1","progress":true,"protocol":1})
}

#[test]
fn reads_the_captured_handshake() {
    let info = parse_initialize(&real()).expect("parses");
    assert_eq!(info.version, "0.3.1");
    assert!(info.progress && info.features.local_model && !info.features.vision);
    assert!(info.has_method("models.list") && !info.has_method("links.list"));
    assert_eq!(info.data_dir, "/home/agent/.local/share");
}

#[test]
fn a_build_without_notifications_or_methods_is_not_usable() {
    let mut v = real();
    v.as_object_mut().unwrap().remove("progress");
    assert!(!parse_initialize(&v).unwrap().progress, "absence is off, not unknown");

    let mut v = real();
    v.as_object_mut().unwrap().remove("methods");
    assert!(parse_initialize(&v).is_err(), "no method list means every call is a guess");

    let mut v = real();
    v.as_object_mut().unwrap().insert("protocol".into(), json!(2));
    assert!(parse_initialize(&v).is_err());
}

#[test]
fn vision_off_means_no_image_search_promises() {
    let info = parse_initialize(&real()).unwrap();
    assert!(!info.features.vision, "released np cannot embed images; the UI must not promise it");
    assert_eq!(MIN_NP, (0, 3, 1));
}
