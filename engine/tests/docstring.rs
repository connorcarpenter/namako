mod test_utils;

use namako_engine::{StatsWriter as _, World as _, given, then};

use test_utils::{World, WorldMut, WorldRef};

#[given("a step with words")]
fn takes_docstring(mut ctx: WorldMut, doc: Option<String>) {
    let _ = ctx.world();
    assert_eq!(doc.as_deref().map(str::trim), Some("hello docstring"));
}

#[given("a step with rows")]
fn takes_datatable(mut ctx: WorldMut, table: Option<Vec<Vec<String>>>) {
    let _ = ctx.world();
    assert_eq!(
        table,
        Some(vec![
            vec!["alpha".to_owned(), "beta".to_owned()],
            vec!["gamma".to_owned(), "delta".to_owned()],
        ])
    );
}

#[given("a step without words")]
fn takes_no_docstring(mut ctx: WorldMut, doc: Option<String>) {
    let _ = ctx.world();
    assert_eq!(doc, None);
}

#[given("a step without rows")]
fn takes_no_datatable(mut ctx: WorldMut, table: Option<Vec<Vec<String>>>) {
    let _ = ctx.world();
    assert_eq!(table, None);
}

#[given("a {word} box")]
fn takes_matches_and_doc(mut ctx: WorldMut, matches: &[String], doc: Option<String>) {
    let _ = ctx.world();
    assert_eq!(matches, &["red".to_owned()]);
    assert_eq!(doc.as_deref().map(str::trim), Some("note"));
}

#[then("the words are seen")]
#[then("the rows are seen")]
fn seen(ctx: WorldRef) {
    let _ = ctx.world();
}

#[given("a step for the note")]
fn noop(mut ctx: WorldMut) {
    let _ = ctx.world();
}

#[then("the recorded note is seen")]
fn note_seen(ctx: WorldRef, doc: Option<String>) {
    let _ = ctx.world();
    assert_eq!(doc.as_deref().map(str::trim), Some("hello docstring"));
}

#[tokio::test]
async fn passes() {
    let writer = World::namako()
        .with_default_cli()
        .run("tests/features/docstring")
        .await;

    assert_eq!(writer.passed_steps(), 12);
    assert_eq!(writer.skipped_steps(), 0);
    assert_eq!(writer.failed_steps(), 0);
    assert_eq!(writer.parsing_errors(), 0);
}
