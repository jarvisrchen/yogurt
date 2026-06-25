use assert_cmd::Command;

#[test]
fn it_prints_help() {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("--help");
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("yogurt"), "help should mention the binary name");
    assert!(
        stdout.contains("start"),
        "help should mention the `start` subcommand"
    );
}
