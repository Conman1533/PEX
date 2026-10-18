//! Scripted test bots for PEX plugins.
//!
//! Usage: `cargo run --release -- <server address> <commands file> <bot name>...`
//!
//! Each bot joins in offline mode. Append lines to the commands file to make a bot act:
//!
//! ```text
//! Alice chat //pos1          chat or run a command (lines starting with / are commands)
//! Bob look 10 64 10          look at a point
//! Bob mine 10 64 10          look at and break a block (gives up after 5 s)
//! Bob use 10 64 10           look at and right-click a block
//! Bob place 10 64 10         same as use: right-click the block to place against it
//! Bob attack Alice           hit another player
//! Bob walk forward 1000      walk for N milliseconds (forward, backward, left, right)
//! Bob pos                    print the bot's position
//! ```
//!
//! Everything the bots see in chat is printed as `[Name] text`, and every action as `[Name] >`.

use azalea::{
    BlockPos, Vec3, WalkDirection, entity::metadata::Player, player::GameProfileComponent,
    prelude::*, swarm::prelude::*,
};
use azalea::ecs::query::With;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Clone, Component, Default)]
struct State;

/// Connected bots by name, so the command reader can find them.
static CLIENTS: std::sync::LazyLock<Arc<Mutex<HashMap<String, Client>>>> =
    std::sync::LazyLock::new(Arc::default);

#[tokio::main]
async fn main() -> AppExit {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: pex-testbot <server> <commands file> <bot name>...");
        return AppExit::error();
    }
    let server = args[1].clone();
    let commands_file = args[2].clone();
    tokio::spawn(run_commands(commands_file, CLIENTS.clone()));

    let accounts = args[3..].iter().map(|name| Account::offline(name)).collect();
    SwarmBuilder::new()
        .add_accounts(accounts)
        .set_handler(handle)
        .join_delay(Duration::from_millis(500))
        .start(server.as_str())
        .await
}

async fn handle(bot: Client, event: Event, _state: State) -> eyre::Result<()> {
    let name = bot.username();
    match event {
        Event::Spawn => {
            println!("[{name}] > spawned");
            CLIENTS.lock().insert(name, bot.clone());
        }
        Event::Chat(chat) => println!("[{name}] {}", chat.message()),
        Event::Disconnect(reason) => println!("[{name}] > disconnected: {reason:?}"),
        _ => {}
    }
    Ok(())
}

/// Polls the commands file and runs each new line on the named bot.
async fn run_commands(path: String, clients: Arc<Mutex<HashMap<String, Client>>>) {
    let _ = tokio::fs::write(&path, "").await;
    let mut done = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let Ok(text) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        for line in lines.iter().skip(done) {
            let mut parts = line.splitn(2, ' ');
            let (Some(name), Some(command)) = (parts.next(), parts.next()) else {
                continue;
            };
            let bot = clients.lock().get(name).cloned();
            match bot {
                Some(bot) => {
                    if let Err(err) = run(&bot, command).await {
                        println!("[{name}] > error: {err}");
                    }
                }
                None => println!("[{name}] > not connected"),
            }
        }
        done = lines.len();
    }
}

fn block(args: &[&str]) -> eyre::Result<BlockPos> {
    let [x, y, z] = args else {
        eyre::bail!("expected x y z");
    };
    Ok(BlockPos::new(x.parse()?, y.parse()?, z.parse()?))
}

fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

async fn run(bot: &Client, command: &str) -> eyre::Result<()> {
    let name = bot.username();
    let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
    let args: Vec<&str> = rest.split_whitespace().collect();
    println!("[{name}] > {command}");
    match verb {
        "chat" => bot.chat(rest),
        "look" => bot.look_at(center(block(&args)?)),
        "mine" => {
            let pos = block(&args)?;
            bot.look_at(center(pos));
            tokio::time::sleep(Duration::from_millis(100)).await;
            if tokio::time::timeout(Duration::from_secs(5), bot.mine(pos))
                .await
                .is_err()
            {
                println!("[{name}] > mining {pos:?} timed out");
            }
        }
        "use" | "place" => {
            let pos = block(&args)?;
            bot.look_at(center(pos));
            tokio::time::sleep(Duration::from_millis(100)).await;
            bot.block_interact(pos);
        }
        "attack" => {
            let target = args.first().copied().unwrap_or_default().to_owned();
            let entity = bot
                .any_entity_by::<&GameProfileComponent, With<Player>>(
                    |profile: &GameProfileComponent| profile.name == target,
                )?
                .ok_or_else(|| eyre::eyre!("can't see {target}"))?;
            bot.attack(entity.id());
        }
        "walk" => {
            let direction = match args.first().copied() {
                Some("backward") => WalkDirection::Backward,
                Some("left") => WalkDirection::Left,
                Some("right") => WalkDirection::Right,
                _ => WalkDirection::Forward,
            };
            let ms: u64 = args.get(1).map_or(Ok(1000), |ms| ms.parse())?;
            bot.walk(direction);
            tokio::time::sleep(Duration::from_millis(ms)).await;
            bot.walk(WalkDirection::None);
        }
        "pos" => println!("[{name}] > at {:?}", bot.position()?),
        _ => eyre::bail!("unknown command {verb}"),
    }
    Ok(())
}
