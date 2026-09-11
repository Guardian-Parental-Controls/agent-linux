use guardian_agent_linux::ipc;

fn init_sentry() -> Option<sentry::ClientInitGuard> {
    let version = option_env!("GUARDIAN_AGENT_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    if let Some(dsn) = option_env!("SENTRY_DSN") {
        if !dsn.is_empty() {
            let options = sentry::ClientOptions {
                release: Some(version.into()),
                auto_session_tracking: true,
                ..Default::default()
            };
            let guard = sentry::init((dsn, options));
            if guard.is_enabled() {
                return Some(guard);
            }
        }
    }
    None
}

#[tokio::main]
async fn main() {
    let _sentry_guard = init_sentry();
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2
        && args
            .iter()
            .any(|arg| arg.starts_with("chrome-extension://"))
    {
        ipc::run_native_messaging_proxy().await;
        return;
    }
    if args.iter().any(|arg| arg == "--active-window-helper") {
        #[cfg(target_os = "linux")]
        {
            match kdotool::get_active_window_info() {
                Ok(info) => {
                    print!("{}", info.title);
                }
                Err(error) => {
                    eprintln!("Error: {error}");
                    std::process::exit(1);
                }
            }
            return;
        }
        #[cfg(not(target_os = "linux"))]
        {
            eprintln!("--active-window-helper is only supported on Linux");
            std::process::exit(1);
        }
    }
    guardian_agent_linux::run_agent().await;
}
