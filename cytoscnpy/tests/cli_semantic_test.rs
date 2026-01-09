use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_cli_semantic_flag() -> Result<()> {
    let temp = TempDir::new()?;
    let py_file = temp.path().join("main.py");
    fs::write(
        &py_file,
        r#"
def foo():
    pass

def bar():
    foo()

if __name__ == "__main__":
    bar()
"#,
    )?;

    let mut cmd = Command::cargo_bin("cytoscnpy-bin")?;
    cmd.arg(temp.path())
        .arg("--semantic")
        .assert()
        .success()
        .stderr(predicate::str::contains("Semantic Analysis Complete"))
        .stderr(predicate::str::contains("Reachable Symbols"));

    Ok(())
}

#[test]
fn test_cli_impact_command() -> Result<()> {
    let temp = TempDir::new()?;
    let py_file = temp.path().join("impact_demo.py");
    fs::write(
        &py_file,
        r#"
def utils():
    pass

def service():
    utils()

def controller():
    service()
"#,
    )?;

    let mut cmd = Command::cargo_bin("cytoscnpy-bin")?;
    cmd.arg("impact")
        .arg("--symbol")
        .arg("impact_demo.utils")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Impact Analysis for 'impact_demo.utils'",
        ))
        .stdout(predicate::str::contains("service"))
        .stdout(predicate::str::contains("controller"));

    Ok(())
}
