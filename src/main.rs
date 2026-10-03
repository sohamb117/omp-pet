mod geometry;
mod hover;
mod install;
mod ipc;
mod metrics;
mod model;
mod native;
mod palette;
mod preferences;
mod sessions;
mod sleep;
mod sprites;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "OMP Pet\n\ncargo run -- [--demo] [--readout]\n      omp-pet --check-pack <folder>\n\nOMP_PET_SOCKET overrides the local socket path."
        );
        return;
    }
    if args.first().is_some_and(|s| s == "--check-pack") {
        let result = args
            .get(1)
            .ok_or_else(|| "Usage: omp-pet --check-pack <folder>".to_string())
            .and_then(|path| {
                sprites::SpritePack::load(
                    std::path::Path::new(path),
                    objc2_foundation::MainThreadMarker::new().unwrap(),
                )
            });
        match result {
            Ok(_) => println!("Sprite pack is valid"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
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
