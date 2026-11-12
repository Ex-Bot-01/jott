//! The About panel, which exists because of one support round trip: a colleague reported "np is too old" and
//! neither the status line nor the warning said *which np file Jott had actually run*. Everything the panel
//! answers has to be answerable from the panel — version, np's version, and where that binary sits.

use jott_core::about::{about, source_label, REPO_URL};
use jott_core::ipc::NpMissingPayload;
use jott_core::np::{NpFeatures, NpInfo, NpSource};
use std::path::{Path, PathBuf};

fn info(version: &str) -> NpInfo {
    NpInfo {
        version: version.into(),
        protocol: 1,
        methods: vec!["search".into()],
        features: NpFeatures { local_model: true, fetch: false, vision: false },
        data_dir: String::new(),
        progress: true,
    }
}

fn problem() -> NpMissingPayload {
    NpMissingPayload {
        message: "np 0.2.4 is too old; Jott needs 0.3.1 or newer".into(),
        install_hint: "install np 0.3.1".into(),
    }
}

const DATA: &str = "/home/tester/.local/share/jott";
const LOG: &str = "/home/tester/.local/share/jott/logs/jott.jsonl";

#[test]
fn the_panel_names_both_versions_and_where_the_binary_came_from() {
    let source = NpSource::Bundled(PathBuf::from("/usr/lib/Jott/np/np"));
    let about = about("0.0.5", Some(&source), Some(&info("0.3.1")), None, Path::new(DATA), Path::new(LOG));
    let json = serde_json::to_value(&about).expect("the panel's payload serialises");

    assert_eq!(json["jottVersion"], "0.0.5");
    assert_eq!(json["np"]["found"], true);
    assert_eq!(json["np"]["version"], "0.3.1");
    assert_eq!(json["np"]["source"], "bundled");
    // The path is the whole point of the panel: it is what turns "np is too old" into a fixable sentence.
    assert_eq!(json["np"]["path"], "/usr/lib/Jott/np/np");
    assert_eq!(json["dataDir"], DATA);
    assert_eq!(json["logPath"], LOG);
    assert_eq!(json["repoUrl"], REPO_URL);
    assert!(json["releasesUrl"].as_str().unwrap().ends_with("/releases"));
    // The copy blob travels with the payload, so the front end cannot invent its own wording.
    assert!(json["diagnostics"].as_str().unwrap().contains("np 0.3.1 from bundled"), "{json}");
}

#[test]
fn a_missing_np_is_a_problem_and_not_version_zero() {
    // The shell keeps a stand-in `(source, info)` pair for "np could not be reached", whose version is
    // 0.0.0 by construction. Showing that as if it were an installed np would send someone hunting for a
    // binary that does not exist.
    let about = about("0.0.5", None, None, Some(&problem()), Path::new(DATA), Path::new(LOG));
    let json = serde_json::to_value(&about).expect("serialises");

    assert_eq!(json["np"]["found"], false);
    assert!(json["np"].get("version").is_none(), "a problem must not look like a version: {json}");
    assert_eq!(json["np"]["problem"], "np 0.2.4 is too old; Jott needs 0.3.1 or newer");
    assert_eq!(json["np"]["hint"], "install np 0.3.1");
}

#[test]
fn the_source_labels_are_the_three_places_jott_looks() {
    let bundled = NpSource::Bundled(PathBuf::from("/opt/jott/np"));
    let override_ = NpSource::Env(PathBuf::from("/home/tester/scratch/np"));
    let on_path = NpSource::Path(PathBuf::from("/usr/local/bin/np"));

    assert_eq!(source_label(&bundled), "bundled");
    // Named exactly as the variable, so someone who set it recognises it at once.
    assert_eq!(source_label(&override_), "$NP_PATH");
    assert_eq!(source_label(&on_path), "$PATH");
}

#[test]
fn copying_details_carries_everything_a_report_needs() {
    let source = NpSource::Path(PathBuf::from("C:\\Tools\\np.exe"));
    let about = about("0.0.5", Some(&source), Some(&info("0.2.4")), None, Path::new(DATA), Path::new(LOG));
    let text = about.diagnostics_text();

    for wanted in ["Jott 0.0.5", "np 0.2.4", "C:\\Tools\\np.exe", "$PATH", DATA, LOG, REPO_URL] {
        assert!(text.contains(wanted), "the copied text is missing {wanted}:\n{text}");
    }
    // Short enough to paste into a message without swallowing it.
    assert!(text.lines().count() <= 8, "too long to paste:\n{text}");
}

#[test]
fn copying_details_works_when_np_was_never_found() {
    let about = about("0.0.5", None, None, Some(&problem()), Path::new(DATA), Path::new(LOG));
    let text = about.diagnostics_text();
    assert!(text.contains("np 0.2.4 is too old"), "{text}");
    assert!(text.contains("install np 0.3.1"), "{text}");
    assert!(text.contains("Jott 0.0.5"), "{text}");
}
