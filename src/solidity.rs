use std::{path::Path, process::Command};

use eyre::bail;

pub fn assembly(sol_path: impl AsRef<Path>) -> eyre::Result<String> {
    let sol_path = sol_path.as_ref();
    if !sol_path.exists() {
        bail!("failed to read Solidity constructor file");
    }

    let output = Command::new("solc")
        .arg(sol_path)
        .arg("--asm")
        .arg("--optimize")
        .output()?;
    let code = String::from_utf8_lossy(&output.stdout);
    if code.is_empty() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("failed to compile the constructor:\n{err}");
    }

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
