use anyhow::Result;
use std::net::SocketAddr;
use yogurt_server::Mode;

pub struct StartArgs {
    pub port: u16,
    pub no_open: bool,
    pub dev: bool,
}

pub async fn run(args: StartArgs) -> Result<()> {
    // Localhost-only bind per D-11.
    let addr: SocketAddr = ([127, 0, 0, 1], args.port).into();
    let mode = if args.dev { Mode::Dev } else { Mode::Release };
    let url = format!("http://127.0.0.1:{}", args.port);

    if !args.no_open {
        // Open in a background task so a failure to spawn the browser doesn't
        // block the server from starting.
        let url_for_open = url.clone();
        tokio::spawn(async move {
            if let Err(e) = open::that(&url_for_open) {
                tracing::warn!(?e, "failed to open browser");
            }
        });
    }

    tracing::info!(%url, "yogurt is starting");
    match yogurt_server::run(addr, mode).await {
        Ok(()) => Ok(()),
        Err(err) => {
            // CONTEXT D-19 / FOUND-06: friendly port-conflict UX. Walk the
            // anyhow chain looking for an `io::Error` with `AddrInUse`.
            if is_addr_in_use(&err) {
                let port = args.port;
                // HI-01: clamp suggestion to valid port range.
                // `port.wrapping_add(1)` on 65535 → 0, which on Unix means
                // "ask for an ephemeral port" -- terrible suggestion. Suggest
                // port-1 at the upper boundary, otherwise port+1.
                let next_port_suggestion = port
                    .checked_add(1)
                    .filter(|p| *p > 0)
                    .map(|p| format!("Try --port {p} or run lsof -i :{port}"))
                    .unwrap_or_else(|| {
                        format!(
                            "No nearby port free -- kill the process holding {port} \
                             with `lsof -i :{port}` and `kill <pid>`"
                        )
                    });
                eprintln!("Port {port} is already in use. {next_port_suggestion}");
                std::process::exit(1);
            }
            Err(err)
        }
    }
}

fn is_addr_in_use(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::AddrInUse {
                return true;
            }
        }
    }
    false
}
