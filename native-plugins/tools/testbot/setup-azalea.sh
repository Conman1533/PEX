#!/usr/bin/env sh
# Builds the Azalea checkout this bot needs: Azalea's 26.3 branch (protocol 777, matching PEX)
# merged with main, whose FixedBitSet rewrite avoids a nightly compiler crash. The 26.3 branch
# alone fails on every nightly tried (2026-09-24 to 2026-10-17).
set -e
cd "$(dirname "$0")"
if [ -d azalea ]; then
    echo "azalea/ already exists; delete it to rebuild"
    exit 0
fi
git clone https://github.com/azalea-rs/azalea azalea
cd azalea
git checkout -b pex-26.3 8041a14706fbdb2b8c481d2e8960d7411e850796
git -c user.email=bot@local -c user.name=bot merge --no-edit 0d38e0dcf47eb258c41b1b09ebc2bcde7253e185 || true
# Keep the 26.3 buffer crate (other 26.3 crates depend on its error types) and main's bitset.
git checkout 8041a14706fbdb2b8c481d2e8960d7411e850796 -- azalea-buf
git checkout 0d38e0dcf47eb258c41b1b09ebc2bcde7253e185 -- azalea-core/src/bitset.rs
# Re-add what the 26.3 branch needs on top of main's bitset (JavaBitSet name, unset, varint form).
git apply ../azalea-26.3-bitset.patch
git add -A
git -c user.email=bot@local -c user.name=bot commit -qm "PEX test bot: 26.3 merged with main"
echo "azalea/ ready; build the bot with: cargo build"
