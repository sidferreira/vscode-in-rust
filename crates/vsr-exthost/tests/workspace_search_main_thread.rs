//! upstream: src/vs/workbench/api/browser/mainThreadWorkspace.ts @ 50e1350fc78d
//!
//! `$startFileSearch` and `$checkExists` driven with synthetic requests, no host (ADR-0089).

use serde_json::json;
use std::path::PathBuf;
use vsr_exthost::main_thread::MainThread;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::proxy_ids::main::MAIN_THREAD_WORKSPACE;
use vsr_rpc::rpc::Call;

fn call(method: &str, args: Vec<Arg>) -> Call {
    Call {
        req: 1,
        rpc_id: MAIN_THREAD_WORKSPACE,
        method: method.into(),
        args,
        cancellable: false,
    }
}

fn uri(path: &std::path::Path) -> serde_json::Value {
    vsr_rpc::handshake::file_uri(path)
}

#[test]
fn start_file_search_lists_each_folder_and_shares_the_limit() {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ws-search-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (one, two) = (base.join("one"), base.join("two"));
    for root in [&one, &two] {
        std::fs::create_dir_all(root).unwrap();
        for f in ["x.txt", "y.txt"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
    }
    let mut mt = MainThread::new();
    mt.workspace = vsr_exthost::workspace_search::WorkspaceService::new(
        vec![one.clone(), two.clone()],
        &json!({}),
        &json!({}),
    );

    let all = mt
        .handle(&call(
            "$startFileSearch",
            vec![
                Arg::Json(json!(null)),
                Arg::Json(json!({ "includePattern": "*.txt" })),
            ],
        ))
        .unwrap();
    let Reply::Json(list) = all else {
        panic!("{all:?}")
    };
    assert_eq!(list.as_array().unwrap().len(), 4, "every folder is listed");
    assert_eq!(
        list[0],
        uri(&one.join("x.txt")),
        "one's files come first, sorted"
    );

    // maxResults is shared across the folders.
    let limited = mt
        .handle(&call(
            "$startFileSearch",
            vec![
                Arg::Json(json!(null)),
                Arg::Json(json!({ "includePattern": "*.txt", "maxResults": 3 })),
            ],
        ))
        .unwrap();
    let Reply::Json(list) = limited else {
        panic!("{limited:?}")
    };
    assert_eq!(list.as_array().unwrap().len(), 3);

    // A folder given explicitly searches only that folder.
    let only_two = mt
        .handle(&call(
            "$startFileSearch",
            vec![
                Arg::Json(uri(&two)),
                Arg::Json(json!({ "includePattern": "*.txt" })),
            ],
        ))
        .unwrap();
    let Reply::Json(list) = only_two else {
        panic!("{only_two:?}")
    };
    assert_eq!(list.as_array().unwrap().len(), 2);

    // $checkExists: true when a folder has a match, false when none does.
    let yes = mt
        .handle(&call(
            "$checkExists",
            vec![Arg::Json(json!([uri(&one)])), Arg::Json(json!(["*.txt"]))],
        ))
        .unwrap();
    assert_eq!(yes, Reply::Json(json!(true)));
    let no = mt
        .handle(&call(
            "$checkExists",
            vec![Arg::Json(json!([uri(&one)])), Arg::Json(json!(["*.rs"]))],
        ))
        .unwrap();
    assert_eq!(no, Reply::Json(json!(false)));
    let _ = std::fs::remove_dir_all(&base);
}
