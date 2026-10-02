//! phone-auth-daemon — BLE + PAM authentication daemon
//!
//! Runs as root on the laptop. Listens for sudo/login requests,
//! pings the paired phone over BLE, and accepts a signed
//! "user verified" token to authorize the PAM session.

mod ble;
mod pairing;
mod pam;
mod auth_request;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "phone-auth-daemon")]
#[command(about = "BLE + PAM auth daemon for phone-based biometric sudo")]
struct Cli {
    /// Path to config directory (default: /etc/phone-auth/)
    #[arg(short, long, default_value = "/etc/phone-auth")]
    config_dir: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon (default)
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(long, default_value_t = false)]
        foreground: bool,
    },
    /// Pair with a new phone device
    Pair,
    /// List paired devices
    List,
    /// Remove a paired device by index
    Remove { index: usize },
    /// Unpair all devices
    UnpairAll,
    /// Run a one-shot auth test (for debugging)
    Test,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phone_auth_daemon=debug,info".into()),
        )
        .init();

    let cli = Cli::parse();

    // Ensure config directory exists
    std::fs::create_dir_all(&cli.config_dir)?;

    match cli.command {
        Some(Commands::Start { foreground }) => {
            if !foreground {
                // Daemonize (simple approach — fork to background)
                daemonize();
            }
            run_daemon(&cli.config_dir).await?;
        }
        Some(Commands::Pair) => {
            pairing::start_pairing(&cli.config_dir).await?;
        }
        Some(Commands::List) => {
            list_pairs(&cli.config_dir)?;
        }
        Some(Commands::Remove { index }) => {
            remove_pair(&cli.config_dir, index)?;
        }
        Some(Commands::UnpairAll) => {
            unpair_all(&cli.config_dir)?;
        }
        Some(Commands::Test) => {
            run_test(&cli.config_dir).await?;
        }
        None => {
            // No subcommand — default to start
            run_daemon(&cli.config_dir).await?;
        }
    }

    Ok(())
}

/// Simple daemonization: fork to background
fn daemonize() {
    // Check if already a daemon (double-fork prevention)
    if unsafe { libc::getppid() == 1 } {
        return;
    }

    match unsafe { libc::fork() } {
        -1 => panic!("fork failed"),
        0 => {
            // Child — become session leader
            unsafe { libc::setsid() };
            // Second fork to prevent controlling terminal
            match unsafe { libc::fork() } {
                -1 => panic!("second fork failed"),
                0 => {
                    // Redirect stdin/stdout/stderr to /dev/null
                    let dev_null = unsafe {
                        libc::open(
                            b"/dev/null\0".as_ptr() as *const libc::c_char,
                            libc::O_RDWR,
                        )
                    };
                    if dev_null >= 0 {
                        unsafe {
                            libc::dup2(dev_null, 0);
                            libc::dup2(dev_null, 1);
                            libc::dup2(dev_null, 2);
                            if dev_null > 2 {
                                libc::close(dev_null);
                            }
                        }
                    }
                }
                _ => std::process::exit(0),
            }
        }
        _ => {
            // Parent exits
            std::process::exit(0);
        }
    }
}

/// Main daemon loop: start BLE server and handle auth requests
async fn run_daemon(config_dir: &std::path::Path) -> anyhow::Result<()> {
    info!("Starting phone-auth daemon");

    // Load paired devices
    let pairing_store = pairing::load_pairs(config_dir)?;
    info!("Loaded {} paired device(s)", pairing_store.devices.len());

    // Create the BLE server (CTAP2 over BLE)
    let ble_server = ble::BleServer::new().await?;
    info!("BLE server started, advertising CTAP2 service");

    // Create the auth request handler
    let auth_handler = auth_request::AuthRequestHandler::new(pairing_store);

    // Start the PAM listener (via pam_exec or D-Bus signal)
    // For now, we expose a simple interface: the daemon listens on a Unix socket
    // for auth requests from the PAM module.

    // Run the BLE event loop
    ble_server.run(&auth_handler).await?;

    Ok(())
}

/// List paired devices
fn list_pairs(config_dir: &std::path::Path) -> anyhow::Result<()> {
    let store = pairing::load_pairs(config_dir)?;
    if store.devices.is_empty() {
        println!("No paired devices.");
        return Ok(());
    }
    for (i, dev) in store.devices.iter().enumerate() {
        println!(
            "[{}] {} (last seen: {})",
            i,
            dev.alias,
            dev.last_seen
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "never".to_string())
        );
    }
    Ok(())
}

/// Remove a paired device
fn remove_pair(config_dir: &std::path::Path, index: usize) -> anyhow::Result<()> {
    let mut store = pairing::load_pairs(config_dir)?;
    if index >= store.devices.len() {
        anyhow::bail!("Invalid device index: {}", index);
    }
    let removed = store.devices.remove(index);
    pairing::save_pairs(config_dir, &store)?;
    println!("Removed: {}", removed.alias);
    Ok(())
}

/// Unpair all devices
fn unpair_all(config_dir: &std::path::Path) -> anyhow::Result<()> {
    pairing::save_pairs(config_dir, &pairing::PairingStore::new())?;
    println!("All devices unpaired.");
    Ok(())
}

/// Run a one-shot auth test
async fn run_test(config_dir: &std::path::Path) -> anyhow::Result<()> {
    let store = pairing::load_pairs(config_dir)?;
    if store.devices.is_empty() {
        anyhow::bail!("No paired devices. Run 'pair' first.");
    }

    info!("Running one-shot auth test...");

    // Create BLE server and try to connect to first paired device
    let _server = ble::BleServer::new().await?;

    // Simulate an auth request
    let request = auth_request::AuthRequest {
        nonce: rand::random(),
        username: "testuser".to_string(),
        timestamp: chrono::Utc::now(),
        ttl: 30,
    };

    info!("Test request: nonce={:016x}", request.nonce);
    info!("In production, the phone would sign this with the pairing key");
    info!("and the daemon would verify the signature before PAM auth");

    Ok(())
}
