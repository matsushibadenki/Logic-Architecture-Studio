#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut flags = std::env::args().skip(1);
    let mut sample = false;
    let mut write_enabled = false;
    let mut http = None;
    let mut security_dir = None;
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--sample" => sample = true,
            "--allow-write" => write_enabled = true,
            "--http" if http.is_none() => {
                http = Some(
                    flags
                        .next()
                        .ok_or("--http requires a port")?
                        .parse::<u16>()?,
                );
            }
            "--http-security" if security_dir.is_none() => {
                security_dir = Some(flags.next().ok_or("--http-security requires a directory")?);
            }
            _ => return Err(format!("Unknown or repeated option: {flag}").into()),
        }
    }
    if http.is_some() != security_dir.is_some() {
        return Err("--http and --http-security must be used together".into());
    }
    studio_service::mcp::run_cli(sample, write_enabled, http.zip(security_dir)).await
}
