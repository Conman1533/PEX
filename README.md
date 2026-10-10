<div align="center">

# Pumpkin (Experimental)

![CI](https://github.com/Pumpkin-MC/Pumpkin/actions/workflows/rust.yml/badge.svg)
[![Discord](https://img.shields.io/discord/1268592337445978193.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/wT8XjrjKkf)
[![License: GPL](https://img.shields.io/badge/License-GPLv3-yellow.svg)](https://opensource.org/licenses/gpl-3-0)

</div>

[Pumpkin](https://pumpkinmc.org/) is a Minecraft server built entirely in Rust, offering a fast, efficient,
and customizable experience. It prioritizes performance and player enjoyment while adhering to the core mechanics of the game. 

This version of it, PEX (Pumkin Experimental) is a hazardously merged version for features that I want. This is not intended for full deployment like the actual properly maintained and merged versions are. Use at your peril!
<div align="center">



![Pumpkin Chunk Loading](https://cdn.discordapp.com/attachments/1045035467772538950/1558367058260008980/pexlogo.png?ex=6acb2deb&is=6ac9dc6b&hm=77d5ab0c78099171a0f8a663349a8e4a1111164bc805fdb207c7d7595365aa9f&)

</div>

## Goals

- **Performance**: Leveraging multi-threading for maximum speed and efficiency.
- **Compatibility**: Supports the latest Java & Bedrock Minecraft server version while adhering to Vanilla game mechanics.
- **Security**: Prioritizes security by preventing known security exploits.
- **Flexibility**: Highly configurable, with the ability to disable unnecessary features.
- **Extensibility**: Provides a foundation for plugin development.

> [!IMPORTANT]
> Pumpkin is currently under heavy development.
>
> [See what needs to be done before the 1.0.0 Release](https://github.com/Pumpkin-MC/Pumpkin/issues/449)

## Features

What PEX adds on top of upstream Pumpkin. Each item was tested on a server before it went in.

### Gameplay fixes
- **Hoes, shovels and axes work again**: tilling farmland, making dirt paths, stripping logs and scraping or waxing copper. A codegen bug had dropped every block transform rule.
- **Projectiles can hit players**: arrows, tridents, snowballs, eggs, fishing bobbers and evoker fangs used to fly straight through players.
- **Items merge into a matching off-hand stack** when picked up. Before, they stayed on the ground.
- **Minecarts climb and roll down slopes**, with vanilla cart physics, cart-to-cart pushing and entity collisions (merged from upstream PR [#3794](https://github.com/Pumpkin-MC/Pumpkin/pull/3794)).

### Mob AI
- **Skeletons pick bow or melee from the weapon they hold**, like vanilla, and switch when it changes. Bogged and Parched shoot more slowly, as in vanilla.
- **Skeletons hide from the sun, sink in water and target iron golems and baby turtles.** Wither skeletons target piglins.
- **Strafing mobs move at vanilla speed.** Skeletons used to zip around at about 3x normal speed.

### Blocks and plants
- **Farmland**: trampling with vanilla chances, pushing entities up when it turns to dirt, surviving under fence gates, and moisture changes visible to clients.
- **Sugar cane and lily pads** use the fluid state, so they work next to and on waterlogged blocks.
- **Crops and stems** follow vanilla light and growth rules.
- **Sea pickles** spread in vanilla's diamond pattern and work with dispensers.
- **Big and small dripleaf**: vanilla growth, collapse and tilt.

### Performance and operations
- **Tick metrics** in `metrics/`:
  - `slow-ticks.log` breaks down every slow tick by world and phase (chunks, players, entities, entity tracking, block entities), with counts and memory use.
  - `summary.csv` records tick times, counts and memory once a minute.
  - Set `PEX_METRICS=0` to turn metrics off and `PEX_SLOW_TICK_MS=<ms>` to change the slow tick threshold (default 100).
- **Chunks unload when players leave**: the server no longer keeps the last player's view-distance chunks loaded forever after they disconnect.
- **Idle servers hand memory back to the OS**: freed memory is returned once a minute on Linux (glibc). With the chunk unloading fix, idle RAM after everyone leaves dropped about 35% in testing (327 MB to 214 MB).
- **Faster surface generation**: the surface rules only look up the biome when a rule needs it, as vanilla does. Output is unchanged.

<!-- Check out our [Github Project](https://github.com/orgs/Pumpkin-MC/projects/3) to see current progress. -->

## How to run

See our [Quick Start](https://docs.pumpkinmc.org/#quick-start) guide to get Pumpkin running.

## Contributions

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md)

## Docs

Pumpkin's documentation can be found at <https://pumpkinmc.org/>

## Communication

Consider joining [our Discord server](https://discord.gg/wT8XjrjKkf) to stay up-to-date on events, updates, and connect with other members.

## Funding

If you want to fund me and help the project, check out the [Donation Page](https://pumpkinmc.org/donate/).

## License & Attribution

* **Pumpkin Server**: Licensed under the [GNU General Public License v3.0 (GPLv3)](LICENSE).
* **Plugin API (`pumpkin-plugin-api` & `pumpkin-plugin-wit`)**: Dual-licensed under [MIT](crates/pumpkin-plugin-api/LICENSE-MIT) OR [Apache-2.0](crates/pumpkin-plugin-api/LICENSE-APACHE) for maximum flexibility when writing plugins.
* **Third-Party Assets & Data**: Bedrock mappings, protocol conversion data, and Minecraft assets are subject to their respective licenses and attribution terms. See [assets/NOTICE.md](assets/NOTICE.md) for full details.
