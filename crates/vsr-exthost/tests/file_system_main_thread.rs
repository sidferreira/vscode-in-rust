//! Upstream source: src/vs/workbench/api/browser/mainThreadFileSystem.ts (driven with synthetic
//! requests, no host). The node host answers stat, readdir, read, write, mkdir and plain deletes
//! itself; these are the operations it forwards, and the ones it would forward from another host.
//! The error names are the `FileSystemProviderErrorCode` values (ADR-0088).

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use vsr_exthost::main_thread::MainThread;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::proxy_ids::main::MAIN_THREAD_FILE_SYSTEM;
use vsr_rpc::rpc::{Call, HandlerError};

fn uri(path: &Path) -> Value {
    json!({ "$mid": 1, "scheme": "file", "authority": "", "path": path.to_string_lossy(), "query": "", "fragment": "" })
}

fn call(method: &str, args: Vec<Arg>) -> Call {
    Call {
        req: 1,
        rpc_id: MAIN_THREAD_FILE_SYSTEM,
        method: method.into(),
        args,
        cancellable: false,
    }
}

fn json_arg(v: Value) -> Arg {
    Arg::Json(v)
}

fn run(mt: &mut MainThread, method: &str, args: Vec<Arg>) -> Result<Reply, HandlerError> {
    mt.handle(&call(method, args))
}

fn err_name(r: Result<Reply, HandlerError>) -> String {
    match r {
        Err(HandlerError::Error { name, .. }) => name,
        other => panic!("expected an error, got {other:?}"),
    }
}

fn json_of(r: Result<Reply, HandlerError>) -> Value {
    match r {
        Ok(Reply::Json(v)) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

struct Fixture {
    mt: MainThread,
    root: PathBuf,
    allowed: PathBuf,
}

/// A scratch folder under the target dir, with `allowed/` as the one destructive root.
fn fixture(name: &str) -> Fixture {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("fs-main-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let allowed = root.join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    let mut mt = MainThread::new();
    mt.file_system.allow_destructive_under(&allowed).unwrap();
    Fixture { mt, root, allowed }
}

#[test]
fn stat_and_readdir_report_types_and_upstream_codes() {
    let mut f = fixture("stat");
    std::fs::write(f.allowed.join("a.txt"), "hello").unwrap();
    std::fs::create_dir_all(f.allowed.join("b")).unwrap();

    let stat = json_of(run(
        &mut f.mt,
        "$stat",
        vec![json_arg(uri(&f.allowed.join("a.txt")))],
    ));
    assert_eq!(stat["type"], 1);
    assert_eq!(stat["size"], 5);
    assert!(
        stat["mtime"].as_u64().unwrap() > 0,
        "mtime is in milliseconds"
    );

    let dir = json_of(run(
        &mut f.mt,
        "$stat",
        vec![json_arg(uri(&f.allowed.join("b")))],
    ));
    assert_eq!(dir["type"], 2);

    let missing = f.allowed.join("nope");
    assert_eq!(
        err_name(run(&mut f.mt, "$stat", vec![json_arg(uri(&missing))])),
        "EntryNotFound"
    );

    let listing = json_of(run(&mut f.mt, "$readdir", vec![json_arg(uri(&f.allowed))]));
    assert_eq!(listing, json!([["a.txt", 1], ["b", 2]]));
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$readdir",
            vec![json_arg(uri(&f.allowed.join("a.txt")))]
        )),
        "EntryNotADirectory"
    );
    let _ = std::fs::remove_dir_all(&f.root);
}

#[test]
fn read_and_write_use_buffers_and_create_parents() {
    let mut f = fixture("rw");
    let target = f.allowed.join("deep").join("x.bin");
    let written = run(
        &mut f.mt,
        "$writeFile",
        vec![json_arg(uri(&target)), Arg::Buffer(vec![0, 1, 2, 255])],
    );
    assert!(matches!(written, Ok(Reply::Empty)), "{written:?}");
    assert_eq!(
        run(&mut f.mt, "$readFile", vec![json_arg(uri(&target))]),
        Ok(Reply::Buffer(vec![0, 1, 2, 255]))
    );
    // Writing over a folder, and reading a folder, are EISDIR.
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$writeFile",
            vec![json_arg(uri(&f.allowed.join("deep"))), Arg::Buffer(vec![1])]
        )),
        "EntryIsADirectory"
    );
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$readFile",
            vec![json_arg(uri(&f.allowed.join("deep")))]
        )),
        "EntryIsADirectory"
    );
    // A `mkdir` where a file is: the plain `Error` upstream throws.
    assert_eq!(
        err_name(run(&mut f.mt, "$mkdir", vec![json_arg(uri(&target))])),
        "Error"
    );
    let _ = std::fs::remove_dir_all(&f.root);
}

#[test]
fn rename_and_copy_refuse_to_overwrite_unless_asked() {
    let mut f = fixture("rename");
    let a = f.allowed.join("a.txt");
    let b = f.allowed.join("b.txt");
    std::fs::write(&a, "A").unwrap();
    std::fs::write(&b, "B").unwrap();
    let no = json!({ "overwrite": false });
    let yes = json!({ "overwrite": true });

    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$rename",
            vec![json_arg(uri(&a)), json_arg(uri(&b)), json_arg(no.clone())]
        )),
        "EntryExists"
    );
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$copy",
            vec![json_arg(uri(&a)), json_arg(uri(&b)), json_arg(no)]
        )),
        "EntryExists"
    );
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$rename",
            vec![
                json_arg(uri(&f.allowed.join("missing"))),
                json_arg(uri(&b)),
                json_arg(yes.clone())
            ]
        )),
        "EntryNotFound"
    );
    assert!(
        run(
            &mut f.mt,
            "$rename",
            vec![json_arg(uri(&a)), json_arg(uri(&b)), json_arg(yes)]
        )
        .is_ok()
    );
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "A");
    assert!(!a.exists());

    // Folders copy recursively; overwriting a folder needs an allowed root.
    let tree = f.allowed.join("tree");
    std::fs::create_dir_all(tree.join("inner")).unwrap();
    std::fs::write(tree.join("inner").join("f.txt"), "f").unwrap();
    let copy_to = f.allowed.join("tree2");
    assert!(
        run(
            &mut f.mt,
            "$copy",
            vec![
                json_arg(uri(&tree)),
                json_arg(uri(&copy_to)),
                json_arg(json!({ "overwrite": false }))
            ]
        )
        .is_ok()
    );
    assert_eq!(
        std::fs::read_to_string(copy_to.join("inner").join("f.txt")).unwrap(),
        "f"
    );
    let _ = std::fs::remove_dir_all(&f.root);
}

#[test]
fn deletes_are_guarded_outside_the_allowed_folders() {
    let mut f = fixture("delete");
    let outside = f.root.join("outside");
    std::fs::create_dir_all(outside.join("inner")).unwrap();
    std::fs::write(outside.join("keep.txt"), "keep").unwrap();
    let inside = f.allowed.join("victim.txt");
    std::fs::write(&inside, "x").unwrap();

    // Trash is refused outside the allowed folders and nothing moves.
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$delete",
            vec![
                json_arg(uri(&outside.join("keep.txt"))),
                json_arg(json!({ "useTrash": true, "recursive": false }))
            ]
        )),
        "Unavailable"
    );
    assert!(outside.join("keep.txt").is_file());

    // A recursive delete of a folder outside is refused; so is one that would need it.
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$delete",
            vec![
                json_arg(uri(&outside)),
                json_arg(json!({ "recursive": true }))
            ]
        )),
        "NoPermissions"
    );
    assert!(outside.join("inner").is_dir());
    // A non-empty folder without `recursive` fails with the host's own error.
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$delete",
            vec![
                json_arg(uri(&outside)),
                json_arg(json!({ "recursive": false }))
            ]
        )),
        "Unknown"
    );

    // Inside the allowed folder, trash is a plain delete; a second delete finds nothing.
    std::fs::remove_file(outside.join("keep.txt")).unwrap();
    assert!(
        run(
            &mut f.mt,
            "$delete",
            vec![
                json_arg(uri(&inside)),
                json_arg(json!({ "useTrash": true }))
            ]
        )
        .is_ok()
    );
    assert!(!inside.exists());
    assert_eq!(
        err_name(run(
            &mut f.mt,
            "$delete",
            vec![json_arg(uri(&inside)), json_arg(json!({}))]
        )),
        "EntryNotFound"
    );
    let _ = std::fs::remove_dir_all(&f.root);
}

#[test]
fn other_schemes_have_no_provider_and_activation_is_a_no_op() {
    let mut f = fixture("schemes");
    let untitled = json!({ "scheme": "untitled", "path": "Untitled-1" });
    assert_eq!(
        err_name(run(&mut f.mt, "$readFile", vec![json_arg(untitled)])),
        "Unavailable"
    );
    assert!(matches!(
        run(
            &mut f.mt,
            "$ensureActivation",
            vec![json_arg(json!("file"))]
        ),
        Ok(Reply::Empty)
    ));
    let _ = std::fs::remove_dir_all(&f.root);
}
