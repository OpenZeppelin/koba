use alloy::primitives::U256;
use bytesize::ByteSize;
use owo_colors::OwoColorize;

pub fn format_gas(gas: U256) -> String {
    let gas: u64 = gas.try_into().unwrap_or(u64::MAX);
    let text = format!("{gas} gas");
    if gas <= 3_000_000 {
        text.bright_green().to_string()
    } else if gas <= 7_000_000 {
        text.yellow().to_string()
    } else {
        text.bright_purple().to_string()
    }
}

/// Pretty-prints a file size based on its limits.
pub fn format_file_size(len: usize, mid: u64, max: u64) -> String {
    let len = ByteSize::b(len as u64);
    let mid = ByteSize::kib(mid);
    let max = ByteSize::kib(max);
    if len <= mid {
        len.bright_green().to_string()
    } else if len <= max {
        len.yellow().to_string()
    } else {
        len.bright_purple().to_string()
    }
}

/// Pretty-prints a data fee.
pub fn format_data_fee(fee: U256) -> eyre::Result<String> {
    let fee: u64 = (fee / U256::from(1e9)).try_into()?;
    let fee: f64 = fee as f64 / 1e9;
    let text = format!("Ξ{fee:.6}");
    let text = if fee <= 5e14 {
        text.bright_green().to_string()
    } else if fee <= 5e15 {
        text.yellow().to_string()
    } else {
        text.bright_purple().to_string()
    };

    Ok(text)
}
