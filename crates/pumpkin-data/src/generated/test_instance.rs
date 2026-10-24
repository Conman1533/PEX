/* This file is generated. Do not edit manually. */
#[allow(clippy::too_many_lines)]
#[allow(clippy::match_same_arms)]
#[allow(clippy::missing_const_for_fn)]
#[allow(clippy::match_single_binding)]
#[must_use]
pub fn get_test_instance_json(path: &str) -> Option<&'static str> {
    match path {
        "minecraft:always_pass" | "always_pass" => Some(include_str!(
            "../../../../assets/datapack/data/minecraft/test_instance/always_pass.json"
        )),
        "pumpkin:creeper_should_run_from_cat" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-unit-test-example/data/pumpkin/test_instance/creeper_should_run_from_cat.json"
        )),
        "pumpkin:redstone/comparator_subtract_clamps" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-dual-run/data/pumpkin/test_instance/redstone/comparator_subtract_clamps.json"
        )),
        "pumpkin:redstone/piston_pushes_redstone_block" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-dual-run/data/pumpkin/test_instance/redstone/piston_pushes_redstone_block.json"
        )),
        "pumpkin:redstone/repeater_lock_holds_off" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-dual-run/data/pumpkin/test_instance/redstone/repeater_lock_holds_off.json"
        )),
        "pumpkin:redstone/torch_double_inverter" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-dual-run/data/pumpkin/test_instance/redstone/torch_double_inverter.json"
        )),
        "pumpkin:redstone/wire_attenuation" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-dual-run/data/pumpkin/test_instance/redstone/wire_attenuation.json"
        )),
        "pumpkin:summon_command_regression" => Some(include_str!(
            "../../../../assets/tests/datapacks/pumpkin-unit-test/data/pumpkin/test_instance/summon_command_regression.json"
        )),
        _ => None,
    }
}
#[must_use]
#[allow(clippy::too_many_lines, clippy::large_stack_arrays)]
pub const fn all_test_instance_names() -> &'static [&'static str] {
    &[
        "minecraft:always_pass",
        "pumpkin:creeper_should_run_from_cat",
        "pumpkin:redstone/comparator_subtract_clamps",
        "pumpkin:redstone/piston_pushes_redstone_block",
        "pumpkin:redstone/repeater_lock_holds_off",
        "pumpkin:redstone/torch_double_inverter",
        "pumpkin:redstone/wire_attenuation",
        "pumpkin:summon_command_regression",
    ]
}
