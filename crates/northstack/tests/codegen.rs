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
    assert!(procs.contains("\"billing.invoices.list\": { kind: \"query\""));
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

#[test]
fn imports_only_what_api_uses() {
    let api_for = |rpc: Rpc<()>| {
        let dir = std::env::temp_dir().join(format!(
            "northstack-codegen-{}",
            northstack::uuid::Uuid::new_v4()
        ));
        rpc.generate_ts(&dir).unwrap();
        let api = std::fs::read_to_string(dir.join("api.ts")).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        api
    };
    // No procedures (e.g. after `just remove-example`): no import at all.
    assert!(!api_for(Rpc::new()).contains("import"));
    // Queries only: importing `mutation` would be an unused import.
    let api = api_for(Rpc::new().query("me", noop));
    assert!(api.contains("import { query } from \"./react\";"), "{api}");
}
