use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;

#[test]
fn help_lists_primary_subcommands() {
    let mut command = Command::cargo_bin("pahcer-web").unwrap();
    command.arg("--help");
    command.assert().success().stdout(
        predicates::str::contains("ui")
            .and(predicates::str::contains("run"))
            .and(predicates::str::contains("-d, --dir <DIRECTORY>"))
            .and(predicates::str::contains("--directory").not())
            .and(predicates::str::contains("-C").not()),
    );
}

#[test]
fn ui_help_lists_global_directory_option() {
    let mut command = Command::cargo_bin("pahcer-web").unwrap();
    command.args(["ui", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicates::str::contains("-d, --dir <DIRECTORY>"));
}

#[test]
fn run_help_lists_global_directory_option() {
    let mut command = Command::cargo_bin("pahcer-web").unwrap();
    command.args(["run", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicates::str::contains("-d, --dir <DIRECTORY>"));
}
