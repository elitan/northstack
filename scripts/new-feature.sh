#!/usr/bin/env bash
# Scaffold a feature module: ./scripts/new-feature.sh <snake_case_name>
set -euo pipefail

name="${1:-}"
if [[ ! "$name" =~ ^[a-z][a-z0-9_]*$ ]]; then
  echo "usage: just new-feature <snake_case_name>" >&2
  exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/server/src/features/$name"
if [[ -e "$dir" ]]; then
  echo "feature $name already exists" >&2
  exit 1
fi
# Procedure prefix in camelCase: user_tags -> userTags
prefix="$(echo "$name" | awk -F_ '{for(i=1;i<=NF;i++) printf "%s", (i==1 ? $i : toupper(substr($i,1,1)) substr($i,2))}')"
type_prefix="$(echo "$name" | awk -F_ '{for(i=1;i<=NF;i++) printf "%s", toupper(substr($i,1,1)) substr($i,2)}')"

mkdir -p "$dir"
cat > "$dir/mod.rs" <<RS
//! ${name}: see features/notes for the full reference layout.

mod db;
mod jobs;
mod procedures;
pub mod types;

use crate::AppState;
use northstack::{Cron, Jobs, Rpc};

pub fn rpc() -> Rpc<AppState> {
    Rpc::new().query("${prefix}.ping", procedures::ping)
}

pub fn jobs() -> Jobs<AppState> {
    Jobs::new()
}

pub fn cron() -> Cron {
    Cron::new()
}
RS
cat > "$dir/types.rs" <<RS
//! Types shared with the frontend.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ${type_prefix}Ping {
    pub ok: bool,
}
RS
cat > "$dir/db.rs" <<RS
//! All SQL for ${name}. Add a migration with \`just migration ${name}\`,
//! run \`just check\` to generate row types, then write queries with \`sqlx::query_as!\`.
RS
cat > "$dir/procedures.rs" <<RS
//! RPC handlers for ${name}.

use northstack::{Ctx, RpcResult};

use super::types::*;
use crate::AppState;

pub async fn ping(_ctx: Ctx<AppState>, _input: ()) -> RpcResult<${type_prefix}Ping> {
    Ok(${type_prefix}Ping { ok: true })
}
RS
cat > "$dir/jobs.rs" <<RS
//! Background jobs for ${name}. Handlers must be idempotent.
RS

# A passing integration test to grow from (more patterns: server/tests/notes.rs).
cat > "$root/server/tests/${name}.rs" <<RS
//! Integration tests for ${name}, through the real HTTP router.
//! Each test gets a fresh database from \`northstack::testing::db\`. More patterns:
//! server/tests/notes.rs (errors, jobs, cron).

use std::time::Duration;

use server::features::${name}::types::*;
use server::{AppState, MIGRATOR};
use northstack::testing::{Client, TestDb};

async fn setup() -> (TestDb, Client) {
    let db = northstack::testing::db(&MIGRATOR).await;
    let router = server::build().router(AppState::new(db.pool()), db.pool(), Duration::from_secs(30));
    (db, Client::new(router))
}

#[tokio::test]
async fn ping() {
    let (_db, client) = setup().await;
    let out: ${type_prefix}Ping = client.call("${prefix}.ping", &()).await.unwrap();
    assert!(out.ok);
}
RS

# Register the module and wire it into build().
echo "pub mod ${name};" >> "$root/server/src/features/mod.rs"
python3 - "$root/server/src/lib.rs" "$name" <<'PY'
import sys
path, name = sys.argv[1], sys.argv[2]
s = open(path).read()
marker = "    // northstack:features"
add = (f"        .rpc(features::{name}::rpc())\n"
       f"        .jobs(features::{name}::jobs())\n"
       f"        .cron(features::{name}::cron())\n")
assert marker in s, "marker comment missing in server/src/lib.rs"
s = s.replace(marker, add + marker, 1)
open(path, "w").write(s)
PY

echo "created server/src/features/${name} and server/tests/${name}.rs; procedure ${prefix}.ping registered"
echo "next: README.md -> 'Building a feature, end to end'. Loop on 'just check', finish with 'just verify'."
