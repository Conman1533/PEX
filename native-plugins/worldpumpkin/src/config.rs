// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CONFIG_FILE_NAME: &str = "config.toml";

pub const PERM_SET: &str = "worldpumpkin:command.set";
pub const PERM_REPLACE: &str = "worldpumpkin:command.replace";
pub const PERM_WALLS: &str = "worldpumpkin:command.walls";
pub const PERM_MOVE: &str = "worldpumpkin:command.move";
pub const PERM_POS: &str = "worldpumpkin:command.pos";
pub const PERM_UNDO: &str = "worldpumpkin:command.undo";
pub const PERM_REDO: &str = "worldpumpkin:command.redo";
pub const PERM_RELOAD: &str = "worldpumpkin:command.reload";
pub const PERM_STATUS: &str = "worldpumpkin:command.status";
pub const PERM_LIMIT_BYPASS: &str = "worldpumpkin:limit.bypass";

#[derive(Clone, Copy)]
pub struct PermissionNode {
    pub node: &'static str,
    pub description: &'static str,
    pub default_op: bool,
}

pub const PERMISSION_NODES: &[PermissionNode] = &[
    PermissionNode {
        node: PERM_SET,
        description: "Allows filling a WorldPumpkin selection.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_REPLACE,
        description: "Allows replacing blocks in a WorldPumpkin selection.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_WALLS,
        description: "Allows building WorldPumpkin selection walls.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_MOVE,
        description: "Allows moving blocks in a WorldPumpkin selection.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_POS,
        description: "Allows setting WorldPumpkin selection positions.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_UNDO,
        description: "Allows undoing WorldPumpkin edits.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_REDO,
        description: "Allows redoing WorldPumpkin edits.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_RELOAD,
        description: "Allows reloading WorldPumpkin config.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_STATUS,
        description: "Allows reading WorldPumpkin status.",
        default_op: true,
    },
    PermissionNode {
        node: PERM_LIMIT_BYPASS,
        description: "Allows bypassing configured WorldPumpkin edit limits.",
        default_op: false,
    },
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub max_blocks_per_operation: u64,
    pub blocks_per_tick: usize,
    pub max_queued_operations: usize,
    pub max_queued_blocks: u64,
    pub max_history_entries: usize,
    pub max_history_blocks: usize,
    pub fast_mode: bool,
    pub notify_clients: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_blocks_per_operation: 250_000,
            blocks_per_tick: 8_192,
            max_queued_operations: 4,
            max_queued_blocks: 1_000_000,
            max_history_entries: 15,
            max_history_blocks: 500_000,
            fast_mode: true,
            notify_clients: true,
        }
    }
}

impl Config {
    /// Loads the config, falling back to defaults when it is invalid so a typo can't disable edits.
    pub fn load(data_folder: &Path) -> Self {
        let config: Self = pex_common::load_config(data_folder, CONFIG_FILE_NAME);
        match config.validate() {
            Ok(()) => config,
            Err(err) => {
                tracing::error!("WorldPumpkin: {err}, using defaults");
                Self::default()
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_blocks_per_operation == 0 {
            return Err("max_blocks_per_operation must be greater than 0".to_owned());
        }
        if self.blocks_per_tick == 0 {
            return Err("blocks_per_tick must be greater than 0".to_owned());
        }
        if self.max_queued_operations == 0 {
            return Err("max_queued_operations must be greater than 0".to_owned());
        }
        if self.max_queued_blocks == 0 {
            return Err("max_queued_blocks must be greater than 0".to_owned());
        }
        if self.max_history_entries == 0 {
            return Err("max_history_entries must be greater than 0".to_owned());
        }
        if self.max_history_blocks == 0 {
            return Err("max_history_blocks must be greater than 0".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        assert!(Config::default().validate().is_ok());
    }

    #[test]
    fn missing_fields_keep_their_defaults() {
        let config: Config = toml::from_str("blocks_per_tick = 10").unwrap_or_default();
        assert_eq!(config.blocks_per_tick, 10);
        assert_eq!(config.max_history_entries, 15);
    }

    #[test]
    fn rejects_zero_limits() {
        let config = Config {
            blocks_per_tick: 0,
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn limit_bypass_is_not_granted_to_ops_by_default() {
        let bypass = PERMISSION_NODES
            .iter()
            .find(|node| node.node == PERM_LIMIT_BYPASS);
        assert!(bypass.is_some_and(|node| !node.default_op));
    }
}
