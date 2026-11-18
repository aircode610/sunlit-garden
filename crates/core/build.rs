// `sqlx::migrate!` and `#[sqlx::test(migrations = …)]` embed the migrations at compile time, but
// cargo only watches the files that already exist: without this, a new migration is left out
// of an incremental build (and of the binary it ships in).
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
