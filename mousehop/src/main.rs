// Build the Windows binary against the GUI subsystem so launching
// mousehop.exe does not open a console window alongside the app — one
// that has to stay open, because closing it kills the process. The
// command-line surface gets its console back at runtime instead; see
// `mousehop::windows_console`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use env_logger::Env;
use input_capture::InputCaptureError;
use input_emulation::InputEmulationError;
use mousehop::{
    capture_test,
    config::{self, Command, Config, ConfigError},
    emulation_test,
    service::{Service, ServiceError},
};
use mousehop_cli::CliError;
use mousehop_ipc::{IpcError, IpcListenerCreationError};
use std::{future::Future, io, process};
use thiserror::Error;
use tokio::task::LocalSet;

#[derive(Debug, Error)]
enum MousehopError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    IpcError(#[from] IpcError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Capture(#[from] InputCaptureError),
    #[error(transparent)]
    Emulation(#[from] InputEmulationError),
    #[error(transparent)]
    Cli(#[from] CliError),
}

fn main() {
    // Windows only, and before anything can log or print: a
    // GUI-subsystem process starts with no standard handles, so hand
    // the console back to invocations that have something to say.
    #[cfg(windows)]
    mousehop::windows_console::attach_parent();

    let role = if std::env::args().skip(1).any(|a| a == "daemon") {
        "daemon"
    } else {
        "gui"
    };
    let env = Env::default().filter_or("MOUSEHOP_LOG_LEVEL", "info");
    env_logger::Builder::from_env(env)
        .format(move |buf, record| {
            use std::io::Write;
            let message = format!(
                "[{} {:<5} {}] {}",
                buf.timestamp(),
                record.level(),
                record.target(),
                record.args()
            );
            // Persist release causes as well as warnings: queued-input cleanup
            // alone cannot identify the event that originally returned control.
            let capture_release = record.level() == log::Level::Info
                && ((record.target() == "mousehop::capture"
                    && message.contains("releasing capture:"))
                    || (record.target() == "mousehop_input_capture"
                        && message.contains("firing AutoRelease")));
            if record.level() <= log::Level::Warn || capture_release {
                mousehop::panic_log::record_warning(role, &message);
            }
            writeln!(buf, "{message}")
        })
        .init();

    // Route panics to a durable logfile before anything else can run.
    // The daemon child's stderr is /dev/null under LaunchServices, so
    // without this a panic (which `panic = "abort"` turns into an
    // immediate process death) leaves no trace anywhere the user can
    // see — which is exactly why the screensaver-lockup crash went
    // undiagnosed. Tag each record by role so a shared log file stays
    // attributable.
    mousehop::panic_log::install(role);

    // On a Linux `cargo install` (no AUR / Flatpak / distro package)
    // the binary alone wouldn't appear in launchers. This silently
    // writes the .desktop entry + icon into ~/.local/share on the
    // first launch, once. Best-effort — never blocks startup.
    #[cfg(all(unix, not(target_os = "macos")))]
    mousehop::desktop_install::ensure_first_launch();

    if let Err(e) = run() {
        log::error!("{e}");
        process::exit(1);
    }
}

fn run() -> Result<(), MousehopError> {
    let config = config::Config::new()?;
    match config.command() {
        Some(command) => match command {
            Command::TestEmulation(args) => run_async(emulation_test::run(config, args))?,
            Command::TestCapture(args) => run_async(capture_test::run(config, args))?,
            Command::Cli(cli_args) => run_async(mousehop_cli::run(cli_args))?,
            Command::Daemon => {
                // if daemon is specified we run the service
                match run_async(run_service(config)) {
                    Err(MousehopError::Service(ServiceError::IpcListen(
                        IpcListenerCreationError::AlreadyRunning,
                    ))) => log::info!("service already running!"),
                    r => r?,
                }
            }
            Command::Firewall(args) => {
                let code = mousehop::firewall::run(config.port(), args.remove, args.dry_run);
                process::exit(code);
            }
            #[cfg(target_os = "macos")]
            Command::AxProbe => {
                // Fresh-process probe of TCC Accessibility state. Spawned
                // by the daemon's TCC.db watcher (see mousehop::tcc_watch
                // on macOS) to bypass cached-trust state in already-running
                // processes — particularly important for the "remove from
                // list" case where AXIsProcessTrusted in the parent keeps
                // reporting cached-true. Exit 0 = granted, 1 = revoked.
                let granted = mousehop::macos_tcc_probe::is_accessibility_granted();
                process::exit(if granted { 0 } else { 1 });
            }
        },
        None => {
            let name = if cfg!(windows) {
                "motionduo-ui.exe"
            } else {
                "motionduo-ui"
            };
            let frontend = std::env::current_exe()?.with_file_name(name);
            if !frontend.is_file() {
                return Err(io::Error::new(io::ErrorKind::NotFound,
                    format!("MyGo frontend missing: {}. Build with scripts/build-mygo.ps1 (or .sh). For headless use: motionduo daemon", frontend.display())).into());
            }
            let status = process::Command::new(frontend)
                .args(std::env::args().skip(1))
                .status()?;
            if !status.success() {
                process::exit(status.code().unwrap_or(1));
            }
        }
    }

    Ok(())
}

fn run_async<F, E>(f: F) -> Result<(), MousehopError>
where
    F: Future<Output = Result<(), E>>,
    MousehopError: From<E>,
{
    // create single threaded tokio runtime
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    // run async event loop
    Ok(runtime.block_on(LocalSet::new().run_until(f))?)
}

async fn run_service(config: Config) -> Result<(), ServiceError> {
    let release_bind = config.release_bind();
    let config_path = config.config_path().to_owned();
    let mut service = Service::new(config).await?;
    log::info!("using config: {config_path:?}");
    log::info!("Press {release_bind:?} to release the mouse");

    // macOS-only: detect AX-permission "remove from list" by polling
    // TCC.db's mtime and confirming via a fresh subprocess. The
    // existing in-process AXIsProcessTrusted polling in the GUI only
    // catches the toggle-off case; the remove case leaves the cached
    // trust state stuck at true forever. See `macos_tcc_watch`.
    //
    // The watcher self-disables when Accessibility was not granted at
    // startup, keeping first-run and unsigned/headless daemons alive with
    // inactive capture/emulation backends. `MOUSEHOP_DISABLE_TCC_WATCH`
    // remains an explicit debugging opt-out for otherwise trusted builds.
    #[cfg(target_os = "macos")]
    if std::env::var_os("MOUSEHOP_DISABLE_TCC_WATCH").is_none() {
        mousehop::macos_tcc_watch::spawn();
    }

    service.run().await?;
    log::info!("service exited!");
    Ok(())
}
