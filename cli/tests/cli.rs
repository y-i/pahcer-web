use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;

#[test]
fn help_lists_primary_subcommands() {
    let mut command = Command::cargo_bin("pahcer-web").unwrap();
    command.arg("--help");
    command
        .assert()
        .success()
        .stdout(predicates::str::contains("ui").and(predicates::str::contains("run")));
}
