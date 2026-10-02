mod geometry;
mod hover;
mod ipc;
mod metrics;
mod model;
mod native;
mod palette;
mod preferences;
mod sessions;
mod sprites;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "OMP Pet\n\ncargo run -- [--demo] [--readout]\n\nOMP_PET_SOCKET overrides the local socket path."
        );
        return;
    }
    let path = ipc::socket_path();
    let server = match ipc::Server::bind(&path) {
        Ok(server) => server,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!("{e}");
            return;
        }
        Err(e) => {
            eprintln!("Could not start OMP Pet at {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    eprintln!("OMP Pet listening at {}", path.display());
    server.start(|event| dispatch2::DispatchQueue::main().exec_async(move || native::event(event)));
    native::run(
        args.iter().any(|s| s == "--demo"),
        args.iter().any(|s| s == "--readout"),
    );
}
