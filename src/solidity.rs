use std::{path::Path, process::Command};

pub fn assembly(sol_path: impl AsRef<Path>) -> eyre::Result<String> {
    let output = Command::new("solc")
        .arg(sol_path.as_ref())
        .arg("--asm")
        .arg("--optimize")
        .output()?;
    let code = String::from_utf8_lossy(&output.stdout);
    let code = code
        .to_string()
        .lines()
        .skip_while(|l| !l.contains("EVM"))
        // Also skip the line containing `EVM`.
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(code)
}
