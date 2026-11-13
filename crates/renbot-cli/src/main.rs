//! renbot da riga di comando, senza interfaccia grafica.

fn main() -> std::process::ExitCode {
    renbot_cli::main_from_env()
}
