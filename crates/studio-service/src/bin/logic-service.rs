fn main() {
    let root = studio_service::workspace_root();
    let result = studio_service::Service::open(&root).and_then(|service| {
        eprintln!(
            "Design Core: {}",
            studio_service::socket_path(&root).display()
        );
        service.listen()
    });
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
