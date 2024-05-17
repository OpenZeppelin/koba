use std::process;

fn main() {
    if let Err(e) = koba::run() {
        eprintln!("Error: {e:?}");
        process::exit(1);
    }
}
