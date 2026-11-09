//! TypeScript generation: api.ts nesting and namespace conflicts.

use northstack::{Ctx, Rpc, RpcResult};

async fn noop(_ctx: Ctx<()>, _input: ()) -> RpcResult<()> {
    Ok(())
}

#[test]
fn generates_nested_api_and_namespaces() {
    let dir = std::env::temp_dir().join(format!(
        "northstack-codegen-{}",
        northstack::uuid::Uuid::new_v4()
    ));
    Rpc::<()>::new()
        .query("billing.invoices.list", noop)
        .mutation("billing.invoices.send", noop)
        .query("me", noop)
        .generate_ts(&dir)
        .unwrap();
    let api = std::fs::read_to_string(dir.join("api.ts")).unwrap();
    assert!(
        api.contains(
            "  billing: {\n    invoices: {\n      list: query(\"billing.invoices.list\"),"
        )
    );
    assert!(api.contains("      send: mutation(\"billing.invoices.send\"),"));
    assert!(api.contains("  me: query(\"me\"),"));
    let procs = std::fs::read_to_string(dir.join("procedures.ts")).unwrap();
    assert!(procs.contains("| \"billing\"") && procs.contains("| \"billing.invoices\""));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[should_panic(expected = "also a namespace")]
fn procedure_cannot_also_be_a_namespace() {
    let dir = std::env::temp_dir().join(format!(
        "northstack-codegen-{}",
        northstack::uuid::Uuid::new_v4()
    ));
    let _ = Rpc::<()>::new()
        .query("notes", noop)
        .query("notes.list", noop)
        .generate_ts(&dir);
}
