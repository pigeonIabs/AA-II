#![cfg_attr(windows, windows_subsystem = "windows")]
mod data;
mod enrichment;
mod host;
mod launcher;
mod refresh;
fn argument(arguments: &[String], key: &str) -> Option<String> {
    arguments
        .iter()
        .position(|v| v == key)
        .and_then(|i| arguments.get(i + 1))
        .cloned()
}
fn run(arguments: &[String]) -> anyhow::Result<()> {
    let root = argument(arguments, "--root")
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or_else(launcher::root)?;
    if arguments
        .iter()
        .any(|v| v == "--refresh-only" || v == "--enrich-parameters")
    {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let store = data::Store::new(root, stop.clone());
        let remote = data::Remote::new(stop);
        let result = if arguments.iter().any(|v| v == "--enrich-parameters") {
            refresh::enrich_saved(&store, &remote)?
        } else {
            refresh::sync(&store, &remote)?
        };
        println!("{result}");
        return Ok(());
    }
    if arguments.iter().any(|v| v == "--serve") {
        host::serve(
            root,
            arguments.iter().any(|v| v == "--desktop"),
            argument(arguments, "--port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4173),
        )
    } else {
        launcher::activate(
            &root,
            arguments.iter().any(|v| v == "--ensure-server"),
            arguments.iter().any(|v| v == "--stop-server"),
        )
    }
}
fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    if let Err(error) = run(&arguments) {
        if !arguments.iter().any(|v| {
            [
                "--ensure-server",
                "--stop-server",
                "--serve",
                "--refresh-only",
                "--enrich-parameters",
            ]
            .contains(&v.as_str())
        }) {
            launcher::show_error(&error.to_string());
        }
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
