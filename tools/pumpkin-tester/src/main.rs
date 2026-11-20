mod bedrock;
mod bot;
mod bot_tests;
mod end_fight;
mod gametests;
mod harness;
mod survival;

use std::{net::SocketAddr, process::ExitCode, sync::Arc, time::Instant};

use bot_tests::block_entity_unload::{self, UnloadedChests};
use pumpkin_core::server::Server;
use pumpkin_gametest::GameTestRunner;
use tokio::net::TcpListener;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

async fn run_all_suites(
    server: &Arc<Server>,
    addr: SocketAddr,
    unloaded_chests: &mut Option<UnloadedChests>,
) -> Result<Vec<String>, String> {
    let mut summary_lines = Vec::new();
    let mut runner = GameTestRunner::new();

    // Survival stages go first so they start from the freshly generated world.
    survival::run_survival_stages(server, addr, &mut summary_lines).await?;
    bot_tests::survival_actions::run_survival_action_tests(server, addr, &mut summary_lines)
        .await?;
    *unloaded_chests =
        Some(bot_tests::run_bot_tests(server, addr, &mut runner, &mut summary_lines).await?);
    gametests::run_gametests(server, &mut runner, &mut summary_lines).await?;

    Ok(summary_lines)
}

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI test runner outputs results to stdout/stderr"
)]
#[tokio::main]
async fn main() -> ExitCode {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("Rayon-Tester-{i}"))
        .build_global();

    let start = Instant::now();
    init_logging();
    if std::env::args().any(|arg| arg == "--bedrock-only") {
        return run_bedrock_only(start).await;
    }
    println!("Starting pumpkin-tester (single server + headless bots + Mojang GameTests)...");

    let temp_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("Failed to create temporary world directory: {err}");
            return ExitCode::FAILURE;
        }
    };

    let server = match harness::create_test_server(&temp_dir).await {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(err) => {
            eprintln!("Failed to bind local TCP listener: {err}");
            return ExitCode::FAILURE;
        }
    };
    let local_addr = match listener.local_addr() {
        Ok(addr) => addr,
        Err(err) => {
            eprintln!("Failed to query local TCP address: {err}");
            return ExitCode::FAILURE;
        }
    };

    let cancel = CancellationToken::new();
    let tasks = TaskTracker::new();
    harness::spawn_java_listener(listener, server.clone(), cancel.clone(), &tasks);

    let mut unloaded_chests = None;
    let mut result = run_all_suites(&server, local_addr, &mut unloaded_chests).await;

    cancel.cancel();
    for player in server.get_all_players() {
        player.remove().await;
        server.remove_player(&player);
    }
    tasks.close();
    tasks.wait().await;
    server.shutdown().await;
    // `stop` saves the worlds with the same `Server::shutdown`. Checked even when a later suite
    // failed, so that failure can't hide this one.
    if let Some(unloaded_chests) = unloaded_chests {
        result = match (
            result,
            block_entity_unload::check_saved_chests(&unloaded_chests),
        ) {
            (Ok(mut lines), Ok(line)) => {
                lines.push(line);
                Ok(lines)
            }
            (Err(err), Ok(line)) => {
                println!("{line}");
                Err(err)
            }
            (Ok(_), Err(err)) => Err(err),
            (Err(err), Err(saved_err)) => Err(format!("{err}\n{saved_err}")),
        };
    }

    // The End fight runs its own servers, which stop and start on their own world folders.
    if let Ok(lines) = &mut result
        && let Err(err) = end_fight::run_end_fight_tests(lines).await
    {
        for line in lines.iter() {
            println!("{line}");
        }
        result = Err(err);
    }

    // Bedrock tests run their own server with Bedrock on.
    if let Ok(lines) = &mut result
        && let Err(err) = Box::pin(bedrock::run_bedrock_tests(lines)).await
    {
        for line in lines.iter() {
            println!("{line}");
        }
        result = Err(err);
    }

    match result {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            println!(
                "All bot integration tests and Mojang GameTests passed in {}ms!",
                start.elapsed().as_millis()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pumpkin-tester FAILED:\n{err}");
            ExitCode::FAILURE
        }
    }
}

/// Server logs on stderr when `PUMPKIN_TESTER_LOG` holds a tracing filter, such as `info`.
fn init_logging() {
    if let Ok(filter) = std::env::var("PUMPKIN_TESTER_LOG") {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr)
            .try_init();
    }
}

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI test runner outputs results to stdout/stderr"
)]
async fn run_bedrock_only(start: Instant) -> ExitCode {
    println!("Starting pumpkin-tester, Bedrock tests only...");
    let mut lines = Vec::new();
    let result = Box::pin(bedrock::run_bedrock_tests(&mut lines)).await;
    for line in lines {
        println!("{line}");
    }
    match result {
        Ok(()) => {
            println!("Bedrock tests passed in {}ms!", start.elapsed().as_millis());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pumpkin-tester FAILED:\n{err}");
            ExitCode::FAILURE
        }
    }
}
