//! Mineable blocks.
//!
//! A [`Block`] is a single cell of a [`Mine`](crate::mine::Mine). Each block
//! carries the four properties the mining loop needs: the [`Material`] it
//! drops, its [`hardness`](Block::hardness) (mining time), the
//! [`World`] it belongs to, and the minimum
//! [`PickaxeTier`] required to break it.
//!
//! Many resources come in two forms: an *ore* variant (drops a single item) and
//! a *dense* variant (`IronBlock`, `Cobblestone`, …) which is tougher and drops
//! nine, mirroring Minecraft's nine-ingots-per-block convention.
//!
//! A dense block is **not** a Compressed unit. This module deals in cells you
//! swing a pickaxe at; a Compressed unit is a denomination the player mints in
//! their inventory, worth a hundred raw, that no block in the ground contains.
//! Nine versus a hundred, mined versus minted — see [`Item`].

use crate::material::{Item, Material};
use crate::pickaxe::PickaxeTier;
use crate::world::World;
use serde::{Deserialize, Serialize};

/// Number of raw items a *dense* block yields when mined.
///
/// Matches Minecraft's crafting ratio: nine ingots or gems make one block, so
/// breaking that block returns nine. **`pub(crate)`, and no wider**: this is a
/// balance number, and callers who want to know what a block is worth should ask
/// the block, via [`Block::drops`].
///
/// It was private until the save audit needed it. [`GameState::validate`](crate::game::GameState)
/// builds the ceiling on what one broken cell can be worth, and the richest cell in
/// the game is a dense block under a maxed Fortune — so the audit has to multiply this
/// by something rather than ask a block, which would only answer for the block it is.
/// That is the one caller the rule above bends for, and it reads the constant to bound
/// a file rather than to pay a player. Unrelated to
/// [`RAW_PER_COMPRESSED`](crate::tunables::RAW_PER_COMPRESSED) (100), which *is*
/// public, because it is a denomination the UI must know in order to render a
/// price — different ratio, different concept, different audience.
pub(crate) const RAW_PER_DENSE_BLOCK: u32 = 9;

/// The most experience any one block is worth, over the whole table.
///
/// **An audit ceiling, not a dial.** It exists for
/// [`GameState::validate`](crate::game::GameState), which bounds the experience a save
/// claims to have earned by the blocks it claims to have broken; a number *below* the
/// table's true maximum would refuse honest saves, which is the one failure that
/// matters here. Written down rather than derived because an enum cannot fold over its
/// own variants, and pinned by `no_block_is_worth_more_experience_than_the_ceiling`,
/// which walks `ALL_BLOCKS` — so a phase-10 re-balance that lifts an arm above it
/// fails a test rather than silently locking someone out of their run.
pub(crate) const MAX_XP_VALUE: u32 = 72;

/// How much [`mining_power`](crate::pickaxe::Pickaxe::mining_power) a block costs
/// per point of [`hardness`](Block::hardness): a cell yields at
/// `hardness * 30`, so it takes `ceil(30 * hardness / mining_power)` ticks.
///
/// This is Minecraft's, and it is the **unit conversion** between two scales that
/// are not the same one — dig speed and hardness. `getDestroyProgress` reads
/// `dig_speed / hardness / 30` per tick, breaking at `1.0`; rearranged so the
/// progress counter carries the power rather than a fraction, the 30 lands here.
/// Without it the two scales are read as one, and a *fresh Wooden pickaxe
/// instamines Stone* — there is no progressive breaking left to speak of.
///
/// **Not a tunable, for the reason the batch-reset threshold is not one.** It is
/// what makes `docs/decisions/0018`'s "1:1 fidelity to Minecraft is kept for hardness"
/// true in practice: the hardness table is only worth porting one-to-one if the
/// break *times* come out one-to-one too, and this is the factor that decides
/// that. Moving it does not tune the game, it revokes the decision. A balance pass
/// that wants faster mining reaches for [`base_power`](crate::pickaxe::PickaxeTier::base_power),
/// which is already Skylode's own curve.
///
/// Minecraft's other divisor — `100`, for mining without the right tool — has no
/// counterpart here and never will: phase 3's mining gate *refuses* a block below
/// the required tier rather than letting the player chip at it. One regime, one
/// constant.
///
/// **It lives beside the hardness it converts, not beside the loop that spends
/// it.** [`Mine::dig`](crate::mine::Mine) was its only reader for four phases, so
/// `mine` was a reasonable home; [`Block::ticks_to_break`] is a second, and it is
/// the one that settles the question. A factor keyed by a block property belongs
/// where that property is defined — the alternative pointed `block` at `mine`, the
/// wrong way down the module chain this crate is layered on.
pub(crate) const TICKS_PER_HARDNESS: f32 = 30.0;

/// A single mineable block.
///
/// Variants are grouped as `<Resource>Ore` / `<Resource>Block` pairs where a
/// dense form exists, plus standalone blocks (Netherrack, Obsidian, …) that have
/// no dual form. The whole enum is `Copy` because a block is just a lightweight
/// tag with no owned data.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Block {
    // --- Overworld ---
    #[default]
    Stone,
    Cobblestone,
    CoalOre,
    CoalBlock,
    IronOre,
    IronBlock,
    GoldOre,
    GoldBlock,
    LapisOre,
    LapisBlock,
    RedstoneOre,
    RedstoneBlock,
    EmeraldOre,
    EmeraldBlock,
    DiamondOre,
    DiamondBlock,
    // --- Nether ---
    Netherrack,
    QuartzOre,
    AncientDebris,
    NetheriteBlock,
    Obsidian,
    CryingObsidian,
    // --- End ---
    Endstone,
    Amethyst,
}

impl Block {
    /// Returns the material the block is made of.
    ///
    /// Total, not partial: *every* block yields something, fillers included.
    /// Netherrack and End Stone drop their own material, exactly as Stone does in
    /// the Overworld — a filler is the block the player breaks most often, and one
    /// that paid nothing would be pure spent time. An `Option` here would hand
    /// every caller a `None` branch that can never be taken.
    pub fn material(self) -> Material {
        match self {
            Self::Stone | Self::Cobblestone => Material::Stone,
            Self::CoalOre | Self::CoalBlock => Material::Coal,
            Self::IronOre | Self::IronBlock => Material::Iron,
            Self::GoldOre | Self::GoldBlock => Material::Gold,
            Self::LapisOre | Self::LapisBlock => Material::Lapis,
            Self::RedstoneOre | Self::RedstoneBlock => Material::Redstone,
            Self::EmeraldOre | Self::EmeraldBlock => Material::Emerald,
            Self::DiamondOre | Self::DiamondBlock => Material::Diamond,
            Self::Netherrack => Material::Netherrack,
            Self::QuartzOre => Material::Quartz,
            Self::AncientDebris | Self::NetheriteBlock => Material::AncientDebris,
            Self::Obsidian => Material::Obsidian,
            Self::CryingObsidian => Material::CryingObsidian,
            Self::Endstone => Material::Endstone,
            Self::Amethyst => Material::Amethyst,
        }
    }

    /// The block's display name, e.g. `"Iron Block"`, `"Quartz Ore"`, `"End Stone"`.
    ///
    /// **A table, and not `format!("{} Ore", material.name())`**, because the
    /// derivation is wrong on three of the twenty-four rows and would be wrong
    /// silently. [`AncientDebris`](Block::AncientDebris)' dense form is *Netherite
    /// Block* — the material is Ancient Debris, the name is not — and
    /// [`Cobblestone`](Block::Cobblestone) and [`Netherrack`](Block::Netherrack)
    /// carry no suffix at all. A block's name belongs to the block, the way
    /// [`MineKind::name`](crate::mine_kind::MineKind::name) belongs to the mine and
    /// not to what it mostly produces.
    ///
    /// Kept apart from [`Material::name`](crate::material::Material::name) for the
    /// reason those two are already apart from a save key: a display name may be
    /// reworded, and the twenty-four rows here reword independently of the fifteen
    /// there. `Block` has no save-key table of its own — a save stores the mine's
    /// kind and its dial, never a grid of block names.
    ///
    /// Read by the Mine screen, whose Break gauge is labelled with the block being
    /// dug (`docs/UI.md` §5.1). That caller reads the name off the grid
    /// cell the target points at rather than storing it, so a name can never
    /// disagree with the cell it is drawn over.
    pub fn name(self) -> &'static str {
        match self {
            Self::Stone => "Stone",
            Self::Cobblestone => "Cobblestone",
            Self::CoalOre => "Coal Ore",
            Self::CoalBlock => "Coal Block",
            Self::IronOre => "Iron Ore",
            Self::IronBlock => "Iron Block",
            Self::GoldOre => "Gold Ore",
            Self::GoldBlock => "Gold Block",
            Self::LapisOre => "Lapis Ore",
            Self::LapisBlock => "Lapis Block",
            Self::RedstoneOre => "Redstone Ore",
            Self::RedstoneBlock => "Redstone Block",
            Self::EmeraldOre => "Emerald Ore",
            Self::EmeraldBlock => "Emerald Block",
            Self::DiamondOre => "Diamond Ore",
            Self::DiamondBlock => "Diamond Block",
            Self::Netherrack => "Netherrack",
            Self::QuartzOre => "Quartz Ore",
            Self::AncientDebris => "Ancient Debris",
            Self::NetheriteBlock => "Netherite Block",
            Self::Obsidian => "Obsidian",
            Self::CryingObsidian => "Crying Obsidian",
            Self::Endstone => "End Stone",
            Self::Amethyst => "Amethyst",
        }
    }

    /// Returns the hardness of the block,
    /// which determines how long it takes to mine.
    ///
    /// Hardness is **not** in the same units as a pickaxe's
    /// [`mining_power`](crate::pickaxe::Pickaxe::mining_power); the two scales are
    /// Minecraft's two scales, and `mine`'s `TICKS_PER_HARDNESS` is the conversion
    /// between them. A block yields after `30 * hardness` of accumulated power, so
    /// it takes `ceil(30 * hardness / mining_power)` ticks. Reading the two as one
    /// scale makes every block in the game an instamine.
    ///
    /// The values are Minecraft's, exactly (Stone `1.5`, Obsidian `50.0`) — this is
    /// the one table `docs/decisions/0018` keeps 1:1, which is what lets the break times
    /// come out 1:1 too and spares us re-deriving a balance pass Mojang already did.
    /// The *dense* forms are the exception, tougher than their ore counterparts —
    /// that toughness is what they cost you for the nine items they give back.
    ///
    /// **Two designed exceptions to the 1:1 rule (phase 10): End Stone `10` and
    /// Amethyst `15`, well above Minecraft's `3` and `1.5`.** The End is already a
    /// *designed* space rather than a ported one — its ore was moved here and gated
    /// behind Netherite, both non-Minecraft choices — so its hardness is a balance
    /// dial too. A soft Amethyst (`1.5`) breaks in two ticks even at Efficiency 5, so
    /// the ten Efficiency levels above it bought nothing: the block was too soft for
    /// speed to matter. Hardening it is what gives Netherite's Efficiency `6..=15` a
    /// reason to exist — a maxed pickaxe farms the End several times faster than a bare
    /// one — and gives the endgame the teeth the mono-mine grind used to supply. Both
    /// stay far below Ancient Debris' `30` and Obsidian's `50`, so the instamine
    /// threshold the Redstone boost guards is untouched. See `docs/decisions/0018` for
    /// the fidelity rule this departs from, and `docs/decisions/0016` for what
    /// Efficiency `6..=15` is for.
    pub fn hardness(self) -> f32 {
        match self {
            Self::Stone => 1.5,
            Self::Cobblestone => 2.0,
            Self::CoalOre => 3.0,
            Self::CoalBlock => 5.0,
            Self::IronOre => 3.0,
            Self::IronBlock => 5.0,
            Self::GoldOre => 3.0,
            Self::GoldBlock => 5.0,
            Self::LapisOre => 3.0,
            Self::LapisBlock => 5.0,
            Self::RedstoneOre => 3.0,
            Self::RedstoneBlock => 5.0,
            Self::EmeraldOre => 3.0,
            Self::EmeraldBlock => 5.0,
            Self::DiamondOre => 3.0,
            Self::DiamondBlock => 5.0,
            Self::Netherrack => 0.4,
            Self::QuartzOre => 3.0,
            Self::AncientDebris => 30.0,
            Self::NetheriteBlock => 50.0,
            Self::Obsidian => 50.0,
            Self::CryingObsidian => 50.0,
            Self::Endstone => 10.0,
            Self::Amethyst => 15.0,
        }
    }

    /// Returns the world in which the block can be found.
    pub fn world(self) -> World {
        match self {
            Self::Stone
            | Self::Cobblestone
            | Self::CoalOre
            | Self::CoalBlock
            | Self::IronOre
            | Self::IronBlock
            | Self::GoldOre
            | Self::GoldBlock
            | Self::LapisOre
            | Self::LapisBlock
            | Self::RedstoneOre
            | Self::RedstoneBlock
            | Self::DiamondOre
            | Self::DiamondBlock
            | Self::EmeraldOre
            | Self::EmeraldBlock => World::Overworld,
            Self::Netherrack
            | Self::QuartzOre
            | Self::AncientDebris
            | Self::NetheriteBlock
            | Self::Obsidian
            | Self::CryingObsidian => World::Nether,
            Self::Endstone | Self::Amethyst => World::End,
        }
    }

    /// Returns the minimum pickaxe tier required to mine the block.
    ///
    /// The gate is what makes pickaxe tier the *mine-opening* axis of the two-axis
    /// progression: a block a tier cannot break cannot stand in a mine that tier can
    /// enter, so the tier the mine gates on is the tier of its cells (see
    /// [`MineKind::gating_tier`](crate::mine_kind::MineKind::gating_tier)). The
    /// endgame ore is gated at the top of the ladder on purpose: **Amethyst — the
    /// prestige currency and the highest enchant material — needs Netherite**, and
    /// the Nether's Quartz needs Diamond, so reaching either is proof of a full
    /// climb rather than of patience alone. This is also what finally gives Netherite
    /// a mine to unlock, where the Overworld's own ladder tops out at Iron.
    pub fn min_pickaxe_tier(self) -> PickaxeTier {
        match self {
            Self::Stone | Self::Cobblestone | Self::CoalOre | Self::CoalBlock => {
                PickaxeTier::Wooden
            }
            Self::IronOre | Self::IronBlock | Self::LapisOre | Self::LapisBlock => {
                PickaxeTier::Stone
            }
            Self::GoldOre
            | Self::GoldBlock
            | Self::RedstoneOre
            | Self::RedstoneBlock
            | Self::DiamondOre
            | Self::DiamondBlock
            | Self::EmeraldOre
            | Self::EmeraldBlock => PickaxeTier::Iron,
            Self::AncientDebris
            | Self::NetheriteBlock
            | Self::Obsidian
            | Self::CryingObsidian
            | Self::Netherrack
            | Self::QuartzOre => PickaxeTier::Diamond,
            Self::Endstone | Self::Amethyst => PickaxeTier::Netherite,
        }
    }

    /// The experience breaking this block grants, before
    /// [Fortune](crate::pickaxe::Pickaxe::fortune_multiplier) — which never multiplies
    /// it (see [`drops`](Block::drops)).
    ///
    /// **Not a function of the drop count, and that is the correction it exists to
    /// make.** XP once *was* the drop count — one per ore cell, nine per dense one —
    /// which tied the level curve to a number chosen for Minecraft's crafting
    /// fidelity, and had a consequence nobody picked: the three endgame mines are
    /// exactly the three with no dense form, so their cells granted one apiece while
    /// an Iron Block granted nine. The Iron mine out-levelled the End, 4 968 XP a full
    /// grid against 3 820. Loot and experience are now two tables, and
    /// [`RAW_PER_DENSE_BLOCK`] governs only the first.
    ///
    /// **A cell of value is worth three times its mine's common cell**, everywhere —
    /// including on the two-material mines, where the rare cell drops a single item.
    /// Three rather than nine so the richness dial still moves the level bar without
    /// the loot ratio dominating it.
    ///
    /// **The mines are ordered, and the ordering is a property rather than a balance
    /// pass.** A full grid is worth `base * (1 + 2w)` for a dial weight `w`, so it is
    /// proportional to the mine's base at *every* dial setting — rising bases order the
    /// twelve mines at all settings at once, which is what
    /// `xp_rises_with_the_progression_at_every_dial_setting` pins. The ordering holds
    /// per grid and per second *with* a boost; it does not hold per second without
    /// one, because Ancient Debris and Obsidian take 67 and 70 seconds a grid where
    /// every other mine takes ten. That is the gap the boost exists to close
    /// (`docs/decisions/0040`), so the one regime where the order breaks is the one the
    /// player is meant to spend Redstone on.
    ///
    /// Read by [`Player::grant_break_experience`](crate::player::Player), which is
    /// where the "before Fortune" above stops being a convention and becomes a
    /// signature — it is handed the blocks a swing broke and no pickaxe at all.
    ///
    /// Provisional; phase 10 balances the twelve bases.
    pub fn xp_value(self) -> u32 {
        match self {
            Self::Stone => 1,
            Self::Cobblestone => 3,
            Self::CoalOre => 2,
            Self::CoalBlock => 6,
            Self::IronOre => 3,
            Self::IronBlock => 9,
            Self::LapisOre => 4,
            Self::LapisBlock => 12,
            Self::GoldOre => 4,
            Self::GoldBlock => 12,
            Self::RedstoneOre => 4,
            Self::RedstoneBlock => 12,
            Self::EmeraldOre => 6,
            Self::EmeraldBlock => 18,
            Self::DiamondOre => 8,
            Self::DiamondBlock => 24,
            Self::Netherrack => 10,
            Self::QuartzOre => 30,
            Self::AncientDebris => 14,
            Self::NetheriteBlock => 42,
            Self::Obsidian => 18,
            Self::CryingObsidian => 54,
            Self::Endstone => 24,
            Self::Amethyst => 72,
        }
    }

    /// Amount of raw material dropped, before
    /// [Fortune](crate::pickaxe::Pickaxe::fortune_multiplier). Dense forms yield
    /// [`RAW_PER_DENSE_BLOCK`]; everything else yields 1.
    ///
    /// Private: it is half an answer. [`drops`](Block::drops) is the whole one,
    /// and pairing the count with the item is what keeps a caller from asking for
    /// the amount and forgetting to ask what it is an amount *of*.
    ///
    /// **Flat across materials, unlike Minecraft**, where Lapis drops 4–9 and
    /// Redstone 4–5. A per-ore count would duplicate the phase-5 cost curve rather
    /// than add to it: prices are quoted in each mine's *own* material, so dropping
    /// four times as much Lapis and charging four times as much for it is the same
    /// game with longer numbers. What distinguishes an ore here is what it buys —
    /// Redstone speed, Emerald Fortune, Lapis enchants — and the ore/dense split
    /// below, which mine richness turns into a dial. Should phase-10 balance want the
    /// variance back, it is a `match` arm here and nothing else: no caller reads a
    /// count it did not get from [`drops`](Block::drops).
    fn drop_amount(self) -> u32 {
        match self {
            Self::Cobblestone
            | Self::CoalBlock
            | Self::IronBlock
            | Self::GoldBlock
            | Self::LapisBlock
            | Self::RedstoneBlock
            | Self::EmeraldBlock
            | Self::DiamondBlock
            | Self::NetheriteBlock => RAW_PER_DENSE_BLOCK,
            _ => 1,
        }
    }

    /// What this block *contains*, before [Fortune]: an item, and how many of it.
    ///
    /// Always an [`Item::Raw`], and that is the point of stating it as an `Item`
    /// at all: nothing in the ground is worth a hundred. A Compressed unit is
    /// minted, a hundred raw at a time, and never dug up.
    ///
    /// This is the block's own drop table, not the outcome of a mining tick. The
    /// [`Excavator`] enchant can *substitute* a Compressed unit for what came out
    /// of the ground, but that is a property of the pickaxe swinging, not of the
    /// rock being swung at — it belongs to the mining loop, which starts here and
    /// then applies the enchants.
    ///
    /// "Contains" is also the word phase 6 grants XP by, and the reason this stays
    /// the pre-Fortune number: experience is paid on what the rock held, loot on
    /// what the pickaxe pulled out. [Fortune] multiplies the second and never the
    /// first, which is what keeps the level axis and the pickaxe axis from becoming
    /// one axis bought twice.
    ///
    /// [`Excavator`]: crate::enchant::EnchantType::Excavator
    /// [Fortune]: crate::pickaxe::Pickaxe::fortune_multiplier
    pub fn drops(self) -> (Item, u32) {
        (Item::Raw(self.material()), self.drop_amount())
    }

    /// How many ticks this block takes to break at `mining_power`, or [`None`] if
    /// it never breaks at all.
    ///
    /// **The closed form of what [`Mine::dig`](crate::mine::Mine) does one tick at a
    /// time.** Progress accumulates at `mining_power` per tick and the cell yields
    /// once it has covered `hardness * TICKS_PER_HARDNESS`, so the count is
    /// `ceil(30 * hardness / mining_power)`. Two implementations of one fact, which
    /// is a liability unless something holds them together — here that is
    /// `a_block_takes_the_ticks_its_hardness_and_the_pickaxe_agree_on` in
    /// [`mine`](crate::mine), which computes the expectation *through this method*
    /// and then digs until the block gives.
    ///
    /// **Public because a price has to be quoted in something the player feels.**
    /// `docs/UI.md` §6.7 states the tier-jump dip as `27 → 100 ticks per block` and
    /// not only as `34.0 → 9.0` of mining power, because nobody has an intuition for
    /// a power figure. A front-end cannot derive the conversion itself:
    /// [`TICKS_PER_HARDNESS`] is `pub(crate)` and stays that way, being a decision
    /// about fidelity rather than a number to render.
    ///
    /// **[`None`] mirrors [`Mine::dig`](crate::mine::Mine)'s own refusal instead of
    /// inventing a sentinel**, and it answers for the same three inputs: a power that
    /// is zero, negative, or not finite. The first two buy no progress; the third is
    /// *refused* rather than honoured, because a `NaN` added to the progress counter
    /// would poison it for the rest of the run and an infinite one is not a swing the
    /// rules are willing to describe. So there is no tick on which the block breaks,
    /// which is exactly what `dig` reports by never yielding. All three are
    /// unreachable through a real pickaxe, since every
    /// [`base_power`](crate::pickaxe::PickaxeTier::base_power) is a positive finite
    /// number — which is precisely why the answer must be a shape a caller cannot
    /// mistake for a very large count of ticks.
    pub fn ticks_to_break(self, mining_power: f32) -> Option<u32> {
        // The same guard `dig` opens with, and for the same reason: a `NaN` compares
        // false against everything, so it would sail past a plain `<= 0.0`.
        if mining_power <= 0.0 || !mining_power.is_finite() {
            return None;
        }
        // `as u32` saturates in Rust rather than wrapping, so a power small enough to
        // overflow the count answers `u32::MAX` — "effectively never" — instead of a
        // small number that would read as an instamine.
        Some((self.hardness() * TICKS_PER_HARDNESS / mining_power).ceil() as u32)
    }
}

/// Every [`Block`] variant, for tests that must cover the whole enum.
///
/// Test-only: an enum has no built-in way to enumerate its variants, and the
/// table-consistency tests (here and in [`world`](crate::world)) need to walk
/// all of them. The `match`es above are exhaustive, so adding a variant already
/// breaks the build; the length assertion in `all_blocks_covers_every_variant`
/// is what reminds you to extend this list too.
#[cfg(test)]
pub(crate) const ALL_BLOCKS: &[Block] = &[
    Block::Stone,
    Block::Cobblestone,
    Block::CoalOre,
    Block::CoalBlock,
    Block::IronOre,
    Block::IronBlock,
    Block::GoldOre,
    Block::GoldBlock,
    Block::LapisOre,
    Block::LapisBlock,
    Block::RedstoneOre,
    Block::RedstoneBlock,
    Block::EmeraldOre,
    Block::EmeraldBlock,
    Block::DiamondOre,
    Block::DiamondBlock,
    Block::Netherrack,
    Block::QuartzOre,
    Block::AncientDebris,
    Block::NetheriteBlock,
    Block::Obsidian,
    Block::CryingObsidian,
    Block::Endstone,
    Block::Amethyst,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The `(ore, dense)` pairs the enum's grouping promises.
    ///
    /// Cobblestone is the odd one out: it is not "a block of Stone" the way
    /// `IronBlock` is a block of Iron. But mechanically it plays exactly that
    /// role — a tougher cell that returns nine Stone — which is why the concept
    /// is named *dense* rather than *block form*. The name has to be true of
    /// every row in this table.
    const DENSE_FORMS: &[(Block, Block)] = &[
        (Block::Stone, Block::Cobblestone),
        (Block::CoalOre, Block::CoalBlock),
        (Block::IronOre, Block::IronBlock),
        (Block::GoldOre, Block::GoldBlock),
        (Block::LapisOre, Block::LapisBlock),
        (Block::RedstoneOre, Block::RedstoneBlock),
        (Block::EmeraldOre, Block::EmeraldBlock),
        (Block::DiamondOre, Block::DiamondBlock),
        (Block::AncientDebris, Block::NetheriteBlock),
    ];

    #[test]
    fn all_blocks_covers_every_variant() {
        assert_eq!(
            ALL_BLOCKS.len(),
            24,
            "a Block variant was added or removed: update ALL_BLOCKS"
        );
    }

    /// [`MAX_XP_VALUE`] must sit at or above every arm of the table, and **one arm
    /// must reach it**.
    ///
    /// The two halves guard opposite failures. Too low, and
    /// [`GameState::validate`](crate::game::GameState) refuses a save an honest player
    /// wrote — the outcome the whole audit is written to avoid. Too high, and the
    /// ceiling stops constraining anything, which is a check that passes forever
    /// without saying so. Only the first is dangerous, which is why the equality is
    /// asserted second and separately.
    #[test]
    fn no_block_is_worth_more_experience_than_the_ceiling() {
        for &block in ALL_BLOCKS {
            assert!(
                block.xp_value() <= MAX_XP_VALUE,
                "{block:?} is worth {} against a ceiling of {MAX_XP_VALUE}",
                block.xp_value()
            );
        }
        assert!(
            ALL_BLOCKS
                .iter()
                .any(|block| block.xp_value() == MAX_XP_VALUE),
            "the ceiling is above every block, so it bounds nothing"
        );
    }

    /// Two blocks sharing a name would make the Break gauge ambiguous about what
    /// is under the pickaxe, which is the one thing that label is for.
    ///
    /// Uniqueness is asserted by counting a set rather than by comparing pairs: the
    /// `match` is exhaustive, so the only way to get this wrong is to paste a row
    /// and forget to edit it, and a duplicate collapses the set by one.
    #[test]
    fn every_block_has_its_own_name() {
        use std::collections::BTreeSet;

        let names: BTreeSet<&str> = ALL_BLOCKS.iter().map(|block| block.name()).collect();
        assert_eq!(
            names.len(),
            ALL_BLOCKS.len(),
            "two blocks answer to the same name"
        );
        for &block in ALL_BLOCKS {
            assert!(!block.name().is_empty(), "{block:?} has no name");
        }
    }

    /// The three rows that stop this table from being `format!("{material} Ore")`.
    ///
    /// Written down because the derivation is *nearly* right, which is what makes it
    /// dangerous: it would produce "Ancient Debris Block" for the block the game
    /// calls Netherite, and would suffix the two fillers that carry no suffix. If
    /// someone ever replaces the table with a format string, this is what says no.
    #[test]
    fn a_blocks_name_is_not_derivable_from_its_material() {
        assert_eq!(Block::NetheriteBlock.material(), Material::AncientDebris);
        assert_eq!(Block::NetheriteBlock.name(), "Netherite Block");

        assert_eq!(Block::Cobblestone.material(), Material::Stone);
        assert_eq!(Block::Cobblestone.name(), "Cobblestone");

        assert_eq!(Block::Netherrack.name(), Material::Netherrack.name());
    }

    /// An ore and its dense form are two shapes of one resource, so they must
    /// agree on everything that identifies the resource. Only the two quantities
    /// that express "dense" — hardness and yield — may differ.
    #[test]
    fn ore_and_dense_forms_describe_the_same_resource() {
        for &(ore, dense) in DENSE_FORMS {
            assert_eq!(
                ore.material(),
                dense.material(),
                "{ore:?} and {dense:?} drop different materials"
            );
            assert_eq!(
                ore.world(),
                dense.world(),
                "{ore:?} and {dense:?} live in different worlds"
            );
            assert_eq!(
                ore.min_pickaxe_tier(),
                dense.min_pickaxe_tier(),
                "{ore:?} and {dense:?} need different pickaxe tiers"
            );
        }
    }

    /// The whole point of the dense form: harder to break, but worth nine times
    /// as much. Either half alone would make it pointless or free.
    #[test]
    fn dense_forms_are_tougher_and_drop_nine() {
        for &(ore, dense) in DENSE_FORMS {
            assert!(
                dense.hardness() > ore.hardness(),
                "{dense:?} ({}) is not tougher than {ore:?} ({})",
                dense.hardness(),
                ore.hardness()
            );
            assert_eq!(ore.drop_amount(), 1, "{ore:?} should drop a single item");
            assert_eq!(
                dense.drop_amount(),
                RAW_PER_DENSE_BLOCK,
                "{dense:?} should drop a full block's worth"
            );
        }
    }

    /// The separation this module exists to keep: a *dense block* sits in the
    /// ground and pays out raw items; a *Compressed unit* is minted by the player
    /// and is worth a hundred. If a block ever *contained* one, the two would have
    /// merged back into a single muddled concept, and the ground would be paying
    /// out a hundred to one where it means to pay nine.
    ///
    /// This constrains the drop *table*, not the mining loop. The Excavator
    /// enchant is allowed to hand the player a Compressed unit — it substitutes
    /// the drop after the fact, which is the pickaxe's doing. What it may not do
    /// is come from the rock.
    #[test]
    fn no_block_contains_a_compressed_unit() {
        for &block in ALL_BLOCKS {
            let (item, amount) = block.drops();
            assert!(
                matches!(item, Item::Raw(_)),
                "{block:?} contains {item}; blocks hold raw items and nothing else"
            );
            assert!(
                amount <= RAW_PER_DENSE_BLOCK,
                "{block:?} drops {amount} items, more than a dense block's worth"
            );
        }
    }

    /// Every block pays. The mining loop has no branch for a swing that lands on
    /// nothing, and this is what entitles it to have none.
    #[test]
    fn every_block_drops_at_least_one_item() {
        for &block in ALL_BLOCKS {
            let (item, amount) = block.drops();
            assert_eq!(item, Item::Raw(block.material()));
            assert!(
                amount >= 1,
                "{block:?} drops nothing, so mining it is a tax"
            );
        }
    }

    /// A cell of value is worth exactly three times its mine's common cell, on all
    /// twelve mines — including the three whose value cell drops a single item, which
    /// is the whole reason XP stopped being the drop count.
    #[test]
    fn a_cell_of_value_is_worth_three_of_its_mines_common_cell() {
        for mine in crate::mine_kind::MineKind::ALL {
            assert_eq!(
                mine.value_block().xp_value(),
                3 * mine.common_block().xp_value(),
                "{mine:?}: the value cell is not worth three of the common one"
            );
        }
    }

    /// The mines grant XP in progression order, and — the load-bearing part — they do
    /// so at **every** richness setting, not merely at the top.
    ///
    /// A full grid is worth `base * (1 + 2w)` for a dial weight `w`, so it stays
    /// proportional to the mine's base whatever the dial does; rising bases therefore
    /// order the twelve mines everywhere at once. That is why this walks three
    /// settings rather than asserting one table: it is testing the *property*, and a
    /// single-point check would pass on a table that inverted somewhere in between.
    #[test]
    fn xp_rises_with_the_progression_at_every_dial_setting() {
        use crate::mine_kind::MineKind;

        // Mines that share a rung are grouped, since ties are deliberate.
        const ORDER: &[&[MineKind]] = &[
            &[MineKind::Stone],
            &[MineKind::Coal],
            &[MineKind::Iron],
            &[MineKind::Lapis, MineKind::Gold, MineKind::Redstone],
            &[MineKind::Emerald],
            &[MineKind::Diamond],
            &[MineKind::Quartz],
            &[MineKind::AncientDebris],
            &[MineKind::Obsidian],
            &[MineKind::Amethyst],
        ];

        // `weight` is the value cell's share of a grid, in percent: the two ends of
        // the dial and a point in between.
        for weight in [10u32, 50, 91] {
            let grid = |mine: MineKind| -> u32 {
                (100 - weight) * mine.common_block().xp_value()
                    + weight * mine.value_block().xp_value()
            };

            for pair in ORDER.windows(2) {
                let (earlier, later) = (pair[0], pair[1]);
                for &a in earlier {
                    for &b in later {
                        assert!(
                            grid(b) > grid(a),
                            "at dial weight {weight}%, {b:?} ({}) grants no more \
                             than {a:?} ({})",
                            grid(b),
                            grid(a)
                        );
                    }
                }
                // Mines sharing a rung must grant exactly the same, or the tie is a
                // typo rather than a decision.
                for &a in earlier {
                    assert_eq!(grid(a), grid(earlier[0]), "{a:?} breaks its rung's tie");
                }
            }
        }
    }

    /// XP and loot are two tables now, and the separation is the point: a dense block
    /// still drops nine but grants three times its ore, so the richness dial moves the
    /// level bar without the crafting ratio deciding the level curve.
    #[test]
    fn xp_is_not_the_drop_count() {
        for &(ore, dense) in DENSE_FORMS {
            assert_eq!(dense.drop_amount(), RAW_PER_DENSE_BLOCK);
            assert_eq!(
                dense.xp_value(),
                3 * ore.xp_value(),
                "{dense:?} grants XP in proportion to its loot rather than its table"
            );
        }
    }

    /// Every block is worth some experience. A cell granting none would make part of
    /// a grid pure spent time on the level axis, the same fault
    /// `every_block_drops_at_least_one_item` rules out on the loot axis.
    #[test]
    fn every_block_grants_some_experience() {
        for &block in ALL_BLOCKS {
            assert!(
                block.xp_value() > 0,
                "{block:?} grants no experience, so mining it is a tax on levelling"
            );
        }
    }

    /// Each world's filler pays out its own material, and the three worlds agree
    /// on that rule. The filler is the block the player breaks most often — most
    /// of a fresh grid *is* filler — so one that yielded nothing would make the
    /// bulk of the game's swings pay nothing.
    #[test]
    fn every_world_filler_drops_its_own_material() {
        assert_eq!(Block::Stone.material(), Material::Stone);
        assert_eq!(Block::Netherrack.material(), Material::Netherrack);
        assert_eq!(Block::Endstone.material(), Material::Endstone);
    }

    #[test]
    fn every_block_has_positive_hardness() {
        // Hardness is the divisor of the mining loop; a zero or negative value
        // would make a block break instantly or never.
        for &block in ALL_BLOCKS {
            assert!(
                block.hardness() > 0.0,
                "{block:?} has a non-positive hardness of {}",
                block.hardness()
            );
        }
    }

    /// The **starter** world's filler must be breakable with the Wooden pickaxe a
    /// fresh player holds, or the opening mine would soft-lock. The deeper worlds'
    /// fillers gate higher — Netherrack behind Diamond, End Stone behind Netherite —
    /// but that is not a soft-lock: the mine's own tier gate refuses entry before a
    /// player can stand in front of a cell they cannot break.
    #[test]
    fn the_starter_filler_is_breakable_with_a_wooden_pickaxe() {
        assert_eq!(Block::Stone.min_pickaxe_tier(), PickaxeTier::Wooden);
    }

    #[test]
    fn the_nether_gates_behind_diamond() {
        for block in [
            Block::Obsidian,
            Block::CryingObsidian,
            Block::AncientDebris,
            Block::NetheriteBlock,
            Block::Netherrack,
            Block::QuartzOre,
        ] {
            assert_eq!(block.min_pickaxe_tier(), PickaxeTier::Diamond);
        }
    }

    /// The End's ore is the top of the gate, which is what gives Netherite a mine to
    /// open — Amethyst is the prestige currency, and reaching it is proof of the full
    /// tier climb.
    #[test]
    fn the_end_gates_behind_netherite() {
        for block in [Block::Endstone, Block::Amethyst] {
            assert_eq!(block.min_pickaxe_tier(), PickaxeTier::Netherite);
        }
    }

    /// **The same number the golden test in [`mine`](crate::mine) reaches by
    /// digging**, and reached here in one multiplication instead of 188 ticks:
    /// Minecraft charges 9.4 seconds for Obsidian with a Diamond pickaxe, which at
    /// 20 tps is 188 ticks. Asserting it on both sides is the point — the count the
    /// Upgrades screen quotes has to be the count the mine actually charges.
    #[test]
    fn obsidian_costs_a_diamond_pickaxe_the_ticks_minecraft_charges() {
        // 8.0 is Diamond's `base_power`, spelled out rather than read off the tier so
        // this test states the pairing it is about instead of tracking a curve.
        assert_eq!(Block::Obsidian.ticks_to_break(8.0), Some(188));
    }

    /// The count **rounds up**, and the boundary is where that matters: a block needs
    /// its full `hardness * 30` of progress, so a tick that lands one point short is
    /// a tick that broke nothing and the player pays another.
    #[test]
    fn a_tick_that_lands_short_still_costs_a_whole_tick() {
        // Stone is hardness 1.5, so 45 points of progress break it.
        assert_eq!(Block::Stone.ticks_to_break(45.0), Some(1), "exactly enough");
        assert_eq!(
            Block::Stone.ticks_to_break(46.0),
            Some(1),
            "more than enough"
        );
        assert_eq!(
            Block::Stone.ticks_to_break(44.0),
            Some(2),
            "one point short"
        );
        assert_eq!(Block::Stone.ticks_to_break(22.5), Some(2), "half, exactly");
    }

    /// A stronger pickaxe never costs more ticks than a weaker one — the property the
    /// whole Upgrades screen is built on, since a rung that raised the count would be
    /// a purchase that made the game slower.
    ///
    /// Walks `ALL_BLOCKS` rather than sampling, for the reason
    /// `a_fresh_wooden_pickaxe_instamines_nothing_in_the_game` does: the claim is
    /// about the game, not about the two blocks a test author thought of.
    #[test]
    fn doubling_the_power_never_costs_more_ticks() {
        for &block in ALL_BLOCKS {
            let (weak, strong) = (block.ticks_to_break(3.0), block.ticks_to_break(6.0));
            assert!(
                weak >= strong,
                "{block:?} took {weak:?} ticks at power 3 and {strong:?} at power 6"
            );
        }
    }

    /// The three inputs that break nothing, and the reason they answer [`None`]
    /// rather than a very large number: a caller printing the count must not be able
    /// to mistake "never" for "eventually". `dig` refuses exactly these.
    #[test]
    fn a_power_that_is_not_positive_and_finite_breaks_nothing() {
        for power in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                Block::Stone.ticks_to_break(power),
                None,
                "power {power} was honoured"
            );
        }
    }
}
