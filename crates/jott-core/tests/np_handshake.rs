use jott_core::np::handshake::{check_version, parse_initialize, MIN_NP};
use jott_core::np::NpMissing;
use serde_json::json;

/// The `initialize` answer captured verbatim from np 0.4.0 on this box (`np serve`, first line of the response),
/// with `"notifications": true` in the request — which is why `progress` appears at all.
fn real() -> serde_json::Value {
    json!({"data_dir":"/home/agent/.local/share",
           "features":{"fetch":true,"local_model":true,"vision":false},
           "methods":["status.get","initialize","search","scan","embed","models.list","links.list","shutdown"],
           "np":"0.4.0","protocol":1,"progress":true})
}

#[test]
fn reads_the_captured_handshake() {
    let info = parse_initialize(&real()).expect("parses");
    assert_eq!(info.version, "0.4.0");
    assert!(info.progress && info.features.local_model && !info.features.vision);
    assert!(info.has_method("models.list") && info.has_method("links.list"), "the 8th method is in the list");
    assert_eq!(info.data_dir, "/home/agent/.local/share");
    // …and it is a build Jott will actually talk to.
    check_version(&info.version).unwrap_or_else(|why| panic!("np 0.4.0 refused: {why}"));
}

/// Kept on purpose: this is the answer the floor now refuses, and the sentence a tester's screenshot shows. The
/// refusal comes from the version check and nothing else — 0.3.1's document is well-formed, it just cannot say
/// whether a folder was ever indexed (#5) or what its model costs (#3). So `parse_initialize` fails on the floor
/// rather than half-parsing it, which is where "too old" belongs: before any request is sent.
#[test]
fn the_0_3_1_answer_is_refused_by_the_floor_and_says_so_as_too_old() {
    let old = json!({"data_dir":"/home/agent/.local/share",
                     "features":{"fetch":true,"local_model":true,"vision":false},
                     "methods":["status.get","initialize","search","scan","embed","models.list","shutdown"],
                     "np":"0.3.1","progress":true,"protocol":1});
    assert!(matches!(parse_initialize(&old), Err(NpMissing::TooOld { found }) if found == "0.3.1"),
            "the refusal names the version it saw, not a generic protocol complaint");
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
    assert_eq!(MIN_NP, (0, 4, 0));
}
