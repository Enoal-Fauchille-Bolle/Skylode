//! The run in progress: the aggregate every other module has been waiting for.
//!
//! Phases 0 to 6 built the rules as *primitives* — a mine that can be dug, a
//! pickaxe that knows its power, an economy that can be spent, a player who can
//! level. What none of them could build is the thing that owns all four at once,
//! and five finished mechanics sat behind
//! `expect(dead_code, reason = "awaiting the phase-7 tick")` for exactly that
//! reason: [`Mine::dig`] charges, [`Boost::tick`] counts down, and neither had a
//! `self` to be called from.
//!
//! [`GameState`] is that `self`, and [`tick`](GameState::tick) is what calls them.
//! This module composes rather than invents: almost nothing here is a new rule,
//! and what it adds is an **order** — the swing resolves impact → procs → XP →
//! loot → refill, and every one of those steps is where it is because an earlier
//! phase wrote down why.
//!
//! **No clock is read here.** [`last_seen`](GameState) is a [`SystemTime`] the
//! *caller* supplies; core never calls `SystemTime::now`. That is the determinism
//! contract, and it is the one invariant in this module the compiler cannot hold
//! on its own — `a_state_reads_no_clock_of_its_own` is what does.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::block::Block;
use crate::boost::Boost;
use crate::economy;
use crate::enchant::EnchantType;
use crate::error::CoreError;
use crate::material::Item;
use crate::mine::{Dug, Mine};
use crate::mine_kind::MineKind;
use crate::player::Player;
use crate::prestige;
use crate::reward::{self, LevelReward, Payout};
use crate::rng::Rng;
use crate::tunables::{
    AUTO_MINER_MILLIBLOCKS_PER_TICK, BOOST_DURATION_TICKS, BOOST_MULTIPLIER, MICROBLOCKS_PER_BLOCK,
    MICROBLOCKS_PER_MILLIBLOCK, MILLIBLOCKS_PER_BLOCK, MILLIS_PER_SECOND, OFFLINE_CAP,
    TICKS_PER_SECOND,
};
use crate::world::World;

/// Everything a saved run consists of.
///
/// The field list is `docs/SYSTEMS.md`'s *saved state*, with one departure and one
/// absence, both deliberate:
///
/// - **The selected mine is not a key into the map.** It is a [`Mine`], owned
///   directly, and the map holds only the mines the player has *left*. A
///   `HashMap<MineKind, Mine>` plus a `selected: MineKind` makes "the mine in front
///   of the player" a lookup that can miss, so every reader — the renderer most of
///   all — would carry an [`Option`] for a value that is never absent, and the
///   crate's lints leave no `unwrap` to dismiss it with. Splitting the two makes
///   the invariant *structural*: [`current_mine`](GameState::current_mine) returns
///   a `&Mine` because there is always one, not because someone checked.
/// - **There is no `prestige` field.** It lives on [`Player`], where phase 8 will
///   find it beside the level it resets. `PHASES.md` lists it here; that is the
///   same class of divergence as "the unlocked worlds are not a field", and it is
///   reconciled in the docs rather than duplicated in the struct.
///
/// The visited mines are created **lazily**, on the first selection. An unvisited
/// mine has no state worth persisting — its grid is a function of its kind and the
/// generator — and building all twelve up front would spend twelve grid draws on
/// mines a run may never open, moving the generator for nothing.
#[derive(Debug)]
pub struct GameState {
    /// Level, experience, pickaxe, inventory and prestige rank.
    player: Player,
    /// The mine the player is in. Always present; see the type-level note.
    mine: Mine,
    /// Every mine the player has entered and left, keyed by kind.
    ///
    /// Holes included: leaving a mine and coming back must find it exactly as it
    /// was left, or switching screens would hand out a free batch reset
    /// (`MECHANICS.md`, *mines persist*).
    visited: HashMap<MineKind, Mine>,
    /// Boost charges bought or granted and not yet fired.
    ///
    /// A plain count, not a collection: every boost in the game is identical, so a
    /// stored one carries no information beyond *how many*.
    /// [`Boost`] is the type of a boost that is **running**.
    boost_charges: u32,
    /// The running boost, if one is.
    active_boost: Option<Boost>,
    /// The auto-miner's unpaid fraction of a **common** cell, in microblocks.
    ///
    /// The carry that makes a fractional rate exact. Without it, a rate below one
    /// block per tick would floor to nothing every tick and the auto-miner would
    /// produce zero forever; with an `f32` instead, the sum of twelve million
    /// increments would stop equalling the product it is supposed to be.
    auto_common_progress: u64,
    /// The auto-miner's unpaid fraction of a **value** cell, in microblocks.
    ///
    /// **Two carries, not one, and the split is why.** A single total, floored into
    /// whole blocks and only then divided by the richness share, loses the value
    /// cell entirely whenever a call is worth fewer than a few blocks: at a 10%
    /// share, one block a tick gives `1 × 10 / 100 = 0` value cells, every tick,
    /// forever. So an idling session would credit only common cells while the same
    /// span credited in one lump — the offline path — would honour the dial. The two
    /// are meant to be the same multiplication, so the share is applied *before* the
    /// floor and each side keeps its own remainder.
    ///
    /// Microblocks rather than milliblocks because that is what the share costs:
    /// `milliblocks × percent ÷ 100` is not an integer, and `× 10` in a unit a
    /// thousand times finer is.
    auto_value_progress: u64,
    /// The unpaid fraction of each item the prestige multiplier owes, in permille.
    ///
    /// The loot half of the same device [`Player`]'s `xp_carry` is the experience half
    /// of: a `×1.2` applied to a block that drops one ore truncates to one, forever, so
    /// the remainder is kept and paid on a later swing (see
    /// [`prestige::apply_with_carry`]).
    ///
    /// **A [`Vec`] and not a [`HashMap`]**, unlike the mines above. The key here is an
    /// [`Item`] and the population is tiny — a swing produces the mine's common block,
    /// its value block, and at most one Excavator substitution — so a linear scan over
    /// three entries beats hashing, and it is the shape
    /// [`credit_auto_mining`](GameState::credit_auto_mining) already builds and
    /// searches for the same reason. It is keyed by [`Item`] rather than by
    /// [`Material`](crate::material::Material) so a Compressed substitution carries its
    /// own remainder: the two denominations are a hundred to one, and merging them
    /// would pay a raw remainder out as a Compressed unit.
    yield_carry: Vec<(Item, u32)>,
    /// The seeded source of every draw the rules make.
    ///
    /// Its *position* is run state, not just its seed: a reloaded run continues its
    /// dice rather than rerolling them, which is why the save stores the generator
    /// and not the number it was built from.
    rng: Rng,
    /// When this run was last written, for the offline accrual phase 7 credits on
    /// resume. Supplied by the caller — see the module header.
    last_seen: SystemTime,
}

impl GameState {
    /// Starts a fresh run: a level-1 player in the Stone mine, nothing bought.
    ///
    /// Takes `now` rather than reading a clock, for the module header's reason. It
    /// takes a `seed` rather than a built [`Rng`] because a front-end has no
    /// business holding the generator — [`Rng`]'s draw methods are `pub(crate)`
    /// precisely so only the rules may advance the sequence.
    ///
    /// The Stone mine is drawn here, so the very first draws of a run lay out its
    /// grid. That is the position every golden vector in this module counts from.
    pub fn new(seed: u64, now: SystemTime) -> Self {
        let mut rng = Rng::from_seed(seed);
        let mine = Mine::new(MineKind::Stone, &mut rng);
        Self {
            player: Player::new(),
            mine,
            visited: HashMap::new(),
            boost_charges: 0,
            active_boost: None,
            auto_common_progress: 0,
            auto_value_progress: 0,
            yield_carry: Vec::new(),
            rng,
            last_seen: now,
        }
    }

    /// Moves the player into `kind`, or refuses with the reason it is shut.
    ///
    /// The two-axis gate, finally enforced. Phase 3 shipped
    /// [`Pickaxe::can_mine`](crate::pickaxe::Pickaxe) as a predicate over a *block*
    /// and said the gate that bites would be the *mine's*; phase 6 shipped
    /// [`Player::mine_lock`] and said the refusal belonged to whatever owned the
    /// selection. This is it, and it is why
    /// [`CoreError::MineLocked`] carries a whole [`MineLock`](crate::mine_kind::MineLock)
    /// rather than a bool.
    ///
    /// **The lock is checked before anything is drawn.** A first visit mints the
    /// mine's grid, which advances the generator; doing that before the gate would
    /// let a refused selection move the position of a run's dice — a refusal that
    /// changes nothing must not change the sequence either. Ordering is the whole
    /// guarantee, and `a_refused_mine_draws_nothing` is what holds it.
    ///
    /// **Re-selecting the mine you are in is a no-op that draws nothing**, the same
    /// shape [`Mine::set_richness_setting`] gives the unmoved dial: the order said
    /// nothing, so nothing — including the generator — may move. Without it, a
    /// front-end re-affirming the selection on every keypress would reroll the
    /// current mine into the map and back.
    ///
    /// The swap is a [`std::mem::replace`], so the mine being left is *moved* into
    /// the map rather than cloned. Its holes travel with it, which is the whole
    /// point of keeping it.
    pub fn select_mine(&mut self, kind: MineKind) -> Result<(), CoreError> {
        if kind == self.mine.kind() {
            return Ok(());
        }

        let lock = self.player.mine_lock(kind);
        if !lock.is_open() {
            return Err(CoreError::MineLocked { kind, lock });
        }

        let incoming = match self.visited.remove(&kind) {
            Some(mine) => mine,
            None => Mine::new(kind, &mut self.rng),
        };
        let outgoing = std::mem::replace(&mut self.mine, incoming);
        self.visited.insert(outgoing.kind(), outgoing);
        Ok(())
    }

    /// The mine the player is in.
    pub fn current_mine(&self) -> &Mine {
        &self.mine
    }

    /// A mine the player has visited, or [`None`] if they never entered it.
    ///
    /// [`None`] is not "no such mine" — the twelve always exist as
    /// [`MineKind`]s — it is "this run has never opened it, so it has no state".
    /// The Mines screen draws an unvisited mine from its kind and its lock, both of
    /// which are answerable without a grid.
    pub fn mine(&self, kind: MineKind) -> Option<&Mine> {
        if kind == self.mine.kind() {
            return Some(&self.mine);
        }
        self.visited.get(&kind)
    }

    /// The player: level, experience, pickaxe, inventory, prestige.
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// Boost charges held and not yet fired.
    pub fn boost_charges(&self) -> u32 {
        self.boost_charges
    }

    /// The running boost, if one is.
    pub fn active_boost(&self) -> Option<&Boost> {
        self.active_boost.as_ref()
    }

    /// When this run was last written.
    pub fn last_seen(&self) -> SystemTime {
        self.last_seen
    }

    /// Slides the current mine's richness dial, refusing above the bought ceiling.
    ///
    /// Forwards to [`Mine::set_richness_setting`], which owns the rule and the
    /// redraw. It is here at all because the dial needs the generator, and the
    /// generator is this struct's.
    pub fn set_richness_setting(&mut self, setting: u32) -> Result<(), CoreError> {
        self.mine.set_richness_setting(setting, &mut self.rng)
    }

    /// Buys one Efficiency level for the pickaxe.
    ///
    /// One of five doors that only hand [`economy`] the borrows it needs. They are
    /// thin on purpose: the rules, the prices and the two-pass debit all live in
    /// that module, and duplicating any of it here would give a purchase two places
    /// to disagree with itself.
    pub fn buy_pickaxe_efficiency(&mut self) -> Result<(), CoreError> {
        let (inventory, pickaxe) = self.player.inventory_and_pickaxe_mut();
        economy::buy_pickaxe_efficiency(inventory, pickaxe)
    }

    /// Buys the jump to the next pickaxe tier, which resets Efficiency.
    pub fn buy_pickaxe_tier(&mut self) -> Result<(), CoreError> {
        let (inventory, pickaxe) = self.player.inventory_and_pickaxe_mut();
        economy::buy_pickaxe_tier(inventory, pickaxe)
    }

    /// Buys one level of an enchant, capped by the highest world reached.
    ///
    /// Supplies the [`World`] itself rather than taking one:
    /// the cap is keyed by the *player's* progress, and a caller free to pass any
    /// world could buy an End-capped Fortune from the Overworld.
    pub fn buy_enchant(&mut self, kind: EnchantType) -> Result<(), CoreError> {
        let world = self.player.highest_unlocked_world();
        let (inventory, pickaxe) = self.player.inventory_and_pickaxe_mut();
        economy::buy_enchant(inventory, pickaxe, kind, world)
    }

    /// Buys the next size level of the current mine, growing and redrawing its grid.
    ///
    /// Three `&mut` borrows out of one `&mut self`, and it compiles only because
    /// they are taken from **distinct fields directly**. Routing any of them through
    /// a `&mut self` method of this struct — `self.rng_mut()` — would borrow the
    /// whole [`GameState`] and the other two would be rejected. That is the same
    /// rule [`Player::inventory_and_pickaxe_mut`] exists to work around one level
    /// down, and the reason this module reaches for its own fields rather than its
    /// own accessors.
    pub fn buy_mine_size(&mut self) -> Result<(), CoreError> {
        economy::buy_mine_size(self.player.inventory_mut(), &mut self.mine, &mut self.rng)
    }

    /// Buys the next richness *ceiling* of the current mine; the dial stays where
    /// it is.
    pub fn buy_mine_richness(&mut self) -> Result<(), CoreError> {
        economy::buy_mine_richness(self.player.inventory_mut(), &mut self.mine)
    }

    /// Buys one boost charge into the reserve.
    ///
    /// [`economy::buy_boost`] only debits — it deliberately does not return a
    /// [`Boost`], since a bought charge is not a running one — so the increment is
    /// this struct's half of the transaction. It happens *after* the debit and only
    /// on success, which is what keeps the refusal free.
    pub fn buy_boost_charge(&mut self) -> Result<(), CoreError> {
        economy::buy_boost(self.player.inventory_mut())?;
        self.boost_charges += 1;
        Ok(())
    }

    /// Fires one charge from the reserve, starting a boost or lengthening the one
    /// already running.
    ///
    /// **Refuses only on an empty reserve.** Firing while a boost runs is legal and
    /// *stacks* — the charge's full duration is added, never swapped in — because
    /// every boost in the game is identical, so the only thing a second charge can
    /// buy is time. See [`Boost::extend`](crate::boost::Boost) for why extending
    /// beats refreshing, and why this rule is permissive on purpose: the
    /// "are you sure?" belongs to the interface, which can see the running boost
    /// through [`active_boost`](GameState::active_boost). A refusal here would put
    /// the question beyond every front-end's reach instead of in front of the
    /// player.
    ///
    /// This is where [`Boost::new`]'s `pub(crate)` pays off: minting the game's
    /// strongest speed multiplier is reachable from exactly one place, and that
    /// place charges a charge for it.
    ///
    /// [`Boost::new`]: crate::boost::Boost
    pub fn fire_boost(&mut self) -> Result<(), CoreError> {
        if self.boost_charges == 0 {
            return Err(CoreError::NoBoostCharge);
        }
        self.boost_charges -= 1;

        match &mut self.active_boost {
            Some(boost) => boost.extend(BOOST_DURATION_TICKS),
            None => self.active_boost = Some(Boost::new(BOOST_MULTIPLIER, BOOST_DURATION_TICKS)),
        }
        Ok(())
    }

    /// Credits the absence since [`last_seen`](GameState::last_seen), and marks the
    /// run as seen at `now`.
    ///
    /// **A multiplication, not a replay.** Seven days is over twelve million ticks,
    /// and the MVP auto-miner is a flat rate — so stepping them would compute a
    /// product the long way, and arrive at the same number. The same
    /// [`credit_auto_mining`](GameState::credit_auto_mining) the tick calls with 1
    /// is called here with the whole span, which is what makes the two provably
    /// equal rather than merely intended to be.
    ///
    /// **`now` is injected, never read.** Core calls no clock; a `SystemTime::now`
    /// here would make every test of this function depend on when it ran, and would
    /// end the crate's determinism contract in one line.
    ///
    /// Three answers, and each is a decision:
    ///
    /// - **A backward clock yields [`None`] and changes nothing.** DST, a timezone
    ///   change and an NTP correction all produce one, and none of them is cheating,
    ///   so `elapsed` clamps to zero and the player is neither penalised nor
    ///   flagged. `None` is exactly the shape §5.7.4 wants: it *skips the modal
    ///   entirely*, because "welcome back, +0" after a DST change is a support ticket
    ///   about a bug that is not one. The stderr line `docs/MECHANICS.md` asks for
    ///   belongs to the front-end — core has no I/O.
    /// - **No elapsed time yields [`None`] too.** The condition §5.7.4 draws on is
    ///   `elapsed > 0`, not "we relaunched", so a run resumed a moment after it was
    ///   written says nothing rather than announcing a zero.
    /// - **A long absence is capped at [`OFFLINE_CAP`], and says so.** A clock set
    ///   far forward earns seven days and no more.
    ///
    /// The wall clock, not a monotonic one, because an absence spans reboots — and
    /// the cap and the clamp are what make its quirks survivable.
    ///
    /// [`OFFLINE_CAP`]: crate::tunables::OFFLINE_CAP
    pub fn resume(&mut self, now: SystemTime) -> Option<OfflineReport> {
        // `duration_since` errs exactly when `now` precedes `last_seen`, which is
        // the backward jump: the `Result` *is* the clamp, so there is no comparison
        // to get the wrong way round.
        let elapsed = now.duration_since(self.last_seen).ok()?;
        self.last_seen = now;
        if elapsed.is_zero() {
            return None;
        }

        let counted = elapsed.min(OFFLINE_CAP);
        // Milliseconds rather than seconds, so a span is not truncated to the second
        // before it is multiplied — the same "ratios before rounding" the
        // auto-miner's two carries exist for.
        let ticks = u64::try_from(counted.as_millis()).unwrap_or(u64::MAX) * TICKS_PER_SECOND
            / MILLIS_PER_SECOND;
        let blocks = ticks * AUTO_MINER_MILLIBLOCKS_PER_TICK / MILLIBLOCKS_PER_BLOCK;

        Some(OfflineReport {
            elapsed,
            counted,
            capped: counted < elapsed,
            blocks,
            gained: self.credit_auto_mining(ticks),
        })
    }

    /// Records that the run was written at `now`, without crediting anything.
    ///
    /// What the autosave calls. [`resume`](GameState::resume) also moves the mark,
    /// but only a session that *starts* is owed a payout — an autosave mid-session
    /// must move the mark so the next absence is measured from it, and must not pay
    /// for the seconds the tick loop has already been paying for. Two calls, because
    /// there are two events.
    pub fn touch(&mut self, now: SystemTime) {
        self.last_seen = now;
    }

    /// Advances the run by one tick, returning what the player should be told.
    ///
    /// **The fixed 20 tps step.** Everything the rules do on a clock happens here
    /// and nowhere else, which is what makes a run reproducible: same seed, same
    /// inputs, same outcome, on any machine and after any reload.
    ///
    /// **It returns events rather than only mutating**, and that is a requirement
    /// from the front-end, not a convenience (`organization/UI-EN.md` §5.6). A
    /// caller that has to *diff the state between frames* to notice an Excavator
    /// proc is guessing: it misses two procs landing in one tick, and it cannot tell
    /// a `+1 Compressed Iron` earned from one the player minted by hand. Six
    /// mechanics owe the player an announcement, one buffer feeds both the toast and
    /// the history, and only the inside of the tick can fill it.
    ///
    /// An ordinary break emits **nothing** — the inventory and the progress bar are
    /// already readable, and a variant per broken cell would allocate on every tick
    /// the player holds Space and hand a maxed Nuke two hundred events to say what
    /// one says. [`Vec::new`] does not allocate, so a quiet tick costs nothing.
    ///
    /// The order inside is the swing's, and every step of it is load-bearing:
    /// **boost → impact → procs → XP → loot → refill**. See
    /// [`resolve_swing`](GameState::resolve_swing).
    pub fn tick(&mut self, input: Input) -> Vec<GameEvent> {
        let mut events = Vec::new();

        // Before the swing, so the power applied below is the one the player still
        // owns. `Boost::multiplier` already returns exactly 1.0 once expired, so
        // this ordering is belt and braces — but the *event* only fires here, and a
        // lapse announced a tick late is a lapse the player saw coming.
        if let Some(boost) = &mut self.active_boost {
            boost.tick();
            if boost.is_expired() {
                self.active_boost = None;
                events.push(GameEvent::BoostExpired);
            }
        }

        // Always, not only when Space is released. `docs/MECHANICS.md` says idle
        // accrual comes *from* the auto-miner, which is about where idle income
        // originates — not about the helper downing tools the moment the player
        // picks theirs up. One that did would tax playing actively.
        self.credit_auto_mining(1);

        if input.space_held {
            self.resolve_swing(&mut events);
        }

        events
    }

    /// One swing: the impact, whatever the enchants make of it, and the payout.
    ///
    /// The order is the whole of phase 7, and each step is where it is for a reason
    /// an earlier phase wrote down:
    ///
    /// 1. **Impact.** [`Mine::dig`] against `mining_power × boost × prestige`. Nothing breaks —
    ///    the usual case, since a block takes many ticks — and the swing is over.
    /// 2. **Spatial procs.** [`Mine::resolve_spatial_procs`] rolls Explosive,
    ///    Jackhammer and Nuke on the impact cell, in a pinned order that is part of
    ///    what a save replays.
    /// 3. **XP, over every block the swing brought down**, blast cells included, in
    ///    **one** grant. One call and not one per block because a swing owes one
    ///    reward per level *crossed*, and two counts could each claim the same level.
    ///    It runs **before Fortune** and takes no pickaxe, so the loot multipliers
    ///    cannot reach it — the rule that keeps the two progression axes apart.
    /// 4. **Loot.** The Excavator rolls once, on the impact block only, and when it
    ///    fires it *replaces* that block's drop and takes no Fortune. Everything else
    ///    pays [`Block::drops`] times
    ///    [`fortune_multiplier`](crate::pickaxe::Pickaxe::fortune_multiplier).
    /// 5. **Refill.** [`Mine::refill_if_empty`] last, because any of steps 1 and 2
    ///    can empty the grid and a refill in the middle would hand the enchants that
    ///    have not rolled yet a fresh two hundred cells.
    ///
    /// Steps 3 and 4 are in that order and not the other way round only for
    /// readability; they touch disjoint state. Steps 1, 2 and 5 are not
    /// interchangeable at all.
    fn resolve_swing(&mut self, events: &mut Vec<GameEvent>) {
        let power = self.player.get_pickaxe().mining_power()
            * self.boost_multiplier()
            * prestige::multiplier(self.player.get_prestige());
        let Some(dug) = self.mine.dig(power, &mut self.rng) else {
            return;
        };

        let procs = self.mine.resolve_spatial_procs(
            dug.cell,
            self.player.get_pickaxe().enchants(),
            &mut self.rng,
        );

        let mut broken = vec![dug.block];
        for proc in &procs {
            broken.extend_from_slice(&proc.broken);
            events.push(GameEvent::SpatialProc {
                kind: proc.kind,
                origin: dug.cell,
                cells: proc.cells.clone(),
            });
        }

        self.grant_experience(&broken, events);
        self.credit_loot(dug, &broken, events);

        if self.mine.refill_if_empty(&mut self.rng) {
            events.push(GameEvent::MineRefilled {
                kind: self.mine.kind(),
            });
        }
    }

    /// Grants the experience `broken` is worth and hands over every level it bought.
    ///
    /// The levels are walked from the one *after* the level held before the grant,
    /// so a lump that crosses several pays each of them: a reward is owed per level
    /// reached, and [`Player::add_experience`] returning the count is what makes that
    /// countable without sampling the level before and after.
    ///
    /// A [`Payout::World`](crate::reward::Payout) needs no action — the unlocked set
    /// is derived from the level, so reaching it *is* the unlock — while
    /// [`Payout::Ore`](crate::reward::Payout) is credited raw, exactly as it is
    /// quoted.
    fn grant_experience(&mut self, broken: &[Block], events: &mut Vec<GameEvent>) {
        let before = self.player.get_level();
        let gained = self.player.grant_break_experience(broken);

        for level in before + 1..=before + gained {
            let reward = reward::reward_for_level(level);
            if let Some(reward) = &reward {
                if let Payout::Ore(lines) = &reward.payout {
                    for &(item, amount) in lines {
                        self.player.inventory_mut().add(item, amount);
                    }
                }
                self.boost_charges += reward.boost_charges;
            }
            events.push(GameEvent::LevelUp { level, reward });
        }
    }

    /// Banks what the swing is worth, with the Excavator's substitution applied to
    /// the impact block alone.
    ///
    /// **The Excavator rolls once per swing, not once per block**, which is the
    /// caller's half of a contract [`Enchants::resolve_excavator`]'s signature cannot
    /// enforce: a maxed Nuke drops two hundred cells in a tick, and rolling each of
    /// them would make the number of draws depend on a blast's geometry — a sequence
    /// no golden vector can pin.
    ///
    /// **A proc replaces the impact drop and takes no Fortune.** The two rarest
    /// levers in the game are kept from composing on purpose, so the substituted
    /// Compressed unit is banked as one and the rest of the swing pays normally.
    ///
    /// **The swing's haul is totalled before the prestige multiplier touches it, and
    /// the multiplier is applied once per item.** Not once per block, and the
    /// difference is the whole mechanic rather than a micro-optimisation: at rank II a
    /// Nuke over two hundred 1-drop cells pays `200 × 1.4 = 280` totalled, and
    /// `200 × ⌊1 × 1.4⌋ = 200` — nothing at all — block by block. What the total
    /// cannot fix on its own is a swing that breaks *one* such cell, so the fraction
    /// left over rides to the next swing in
    /// [`yield_carry`](GameState); see [`prestige::apply_with_carry`].
    ///
    /// [`Enchants::resolve_excavator`]: crate::enchant::Enchants
    fn credit_loot(&mut self, dug: Dug, broken: &[Block], events: &mut Vec<GameEvent>) {
        let fortune = self.player.get_pickaxe().fortune_multiplier();
        let excavated = self
            .player
            .get_pickaxe()
            .enchants()
            .resolve_excavator(dug.block.material(), &mut self.rng);

        // Skip the impact block in the ordinary payout exactly when the Excavator
        // took it over: `skip(1)` reaches the right element because `resolve_swing`
        // pushes the impact first and the blast cells after it.
        let mut haul: Vec<(Item, u32)> = Vec::new();
        for block in broken.iter().skip(usize::from(excavated.is_some())) {
            let (item, amount) = block.drops();
            *entry(&mut haul, item) += amount * fortune;
        }

        if let Some(item) = excavated {
            *entry(&mut haul, item) += 1;
            events.push(GameEvent::ExcavatorProc { item });
        }

        let permille = prestige::multiplier_permille(self.player.get_prestige());
        for (item, amount) in haul {
            let paid =
                prestige::apply_with_carry(amount, permille, entry(&mut self.yield_carry, item));
            // A swing whose whole yield is still sitting in the carry banks nothing;
            // `Inventory::add` would take a zero happily, but the skip keeps the
            // inventory's own history free of entries that changed nothing.
            if paid > 0 {
                self.player.inventory_mut().add(item, paid);
            }
        }
    }

    /// The multiplier a running boost contributes, or exactly `1.0` when none is.
    fn boost_multiplier(&self) -> f32 {
        self.active_boost.as_ref().map_or(1.0, Boost::multiplier)
    }

    /// Credits what the basic auto-miner produced over `ticks`, and returns it.
    ///
    /// **A multiplication, not a simulation**, and the *same* multiplication online
    /// and offline — the tick calls it with 1, and the offline accrual with however
    /// many ticks an absence was worth. `docs/MECHANICS.md` argues the case
    /// for the offline half (seven days is over twelve million ticks, and a flat
    /// rate makes stepping them a multiplication done the long way); making the
    /// online half the same code is what stops the two from drifting into two
    /// balance passes.
    ///
    /// **It never touches the grid**, so an idle mine does not visibly empty. That
    /// is the price of one model instead of two, and what it buys is that a player
    /// who leaves for a week and one who watches for a week are paid identically —
    /// which under a grid-walking online path they would not be.
    ///
    /// **It draws no randomness**, and cannot: it reads the *expected* composition
    /// from [`value_weight_percent`](Mine::value_weight_percent) rather than
    /// sampling cells. That is what leaves the generator's position a function of
    /// the player's swings alone, and it is also why the spatial enchants and the
    /// Excavator are unreachable from here — `docs/DECISIONS.md` settles that procs
    /// fire on active mining only, and a path that cannot draw cannot proc.
    ///
    /// **It grants no experience.** Ore opens pickaxes and levels open worlds, and
    /// an auto-miner that paid experience would open the Nether and the End over a
    /// long absence — collapsing the two-axis gating into a clock. It is the same
    /// rule that keeps Fortune off the experience, applied to elapsed time instead
    /// of to an upgrade. The consequence that makes it cheap: nothing here can cross
    /// a level, so the whole level-up cascade stays in the interactive swing with
    /// exactly one caller.
    ///
    /// Integer arithmetic with a carried remainder **per cell kind**, so a
    /// fractional rate loses nothing across a session: 100 calls of one tick credit
    /// exactly what one call of 100 does. That identity is the whole licence for the
    /// offline path, and it is what
    /// `a_span_credited_at_once_pays_what_the_same_span_pays_tick_by_tick` holds —
    /// see [`auto_value_progress`](GameState) for the split that a single carry got
    /// wrong.
    ///
    /// **The prestige multiplier lands on the *rate*, once**, and this is the one path
    /// in the game where it needs no carry of its own: the microblock unit is finer
    /// than the permille denominator, so scaling the rate before the split is exact,
    /// and the two carries that already exist absorb whatever fraction of a cell it
    /// produces. Once and not twice — an auto-miner that took the multiplier as
    /// *speed* and again as *yield* would pay `×1.96` at rank II, making an absence
    /// the best use of a rank the player just bought with a run.
    fn credit_auto_mining(&mut self, ticks: u64) -> Vec<(Item, u32)> {
        let permille = u64::from(prestige::multiplier_permille(self.player.get_prestige()));
        let milliblocks = ticks.saturating_mul(AUTO_MINER_MILLIBLOCKS_PER_TICK);
        let value_share = u64::from(self.mine.value_weight_percent());
        // `× MICROBLOCKS_PER_MILLIBLOCK` before `÷ 100` keeps the share exact: the
        // finer unit is what makes `milliblocks × percent ÷ 100` an integer. The
        // prestige scaling joins the same numerator and divides last, for the same
        // reason — an early `÷ PERMILLE` would round the rate down every call, and 100
        // calls of one tick would stop equalling one call of 100.
        let per_percent = milliblocks
            .saturating_mul(MICROBLOCKS_PER_MILLIBLOCK)
            .saturating_mul(permille)
            / u64::from(prestige::PERMILLE)
            / 100;
        self.auto_value_progress += per_percent.saturating_mul(value_share);
        self.auto_common_progress += per_percent.saturating_mul(100 - value_share);

        let kind = self.mine.kind();
        let fortune = u64::from(self.player.get_pickaxe().fortune_multiplier());

        let mut gained: Vec<(Item, u32)> = Vec::new();
        for (block, progress) in [
            (kind.common_block(), &mut self.auto_common_progress),
            (kind.value_block(), &mut self.auto_value_progress),
        ] {
            let count = *progress / MICROBLOCKS_PER_BLOCK;
            *progress -= count * MICROBLOCKS_PER_BLOCK;
            let (item, per_block) = block.drops();
            let amount = count
                .saturating_mul(u64::from(per_block))
                .saturating_mul(fortune);
            if amount == 0 {
                continue;
            }
            // The two-material mines drop two different items; the same-material
            // ones drop the same item twice, and must merge rather than list it
            // twice — the inventory would not care, but a caller rendering the
            // offline summary would print the same line back to back.
            let amount = u32::try_from(amount).unwrap_or(u32::MAX);
            let total = entry(&mut gained, item);
            *total = total.saturating_add(amount);
        }

        for &(item, amount) in &gained {
            self.player.inventory_mut().add(item, amount);
        }
        gained
    }

    /// Trades the whole run for one prestige rank, or refuses without touching it.
    ///
    /// The endgame loop `docs/MECHANICS.md` specifies, and the last mechanic the core
    /// owed the game. Everything the player has bought goes back to the start —
    /// pickaxe, enchants, inventory, every mine's size and richness, the mining level
    /// itself — and what survives is the rank and the permanent multiplier it grants
    /// on ore yield, mining speed and experience ([`prestige`]).
    ///
    /// ## The order, and why nothing here is interchangeable
    ///
    /// 1. **The level gate**, refusing with [`CoreError::PrestigeLocked`]. Amethyst
    ///    only drops in the End, so a player who cannot reach it can never pay — but
    ///    checking solvency first would answer "you need 512 Amethyst" to someone
    ///    thirty levels from the ore, which is the wrong sentence
    ///    (`docs/UI.md` §6.8 says so, and prints the level gap instead).
    /// 2. **The price**, through [`economy::pay`] — the same two-pass till every
    ///    purchase in the game uses, so an unaffordable prestige debits nothing. The
    ///    debit is *superseded* a line later by the wipe, and it is still routed
    ///    through the till rather than through [`economy::can_afford`]: one
    ///    implementation of "can they pay", and the refusal arrives already carrying
    ///    the `needed` and `held` a preview wants.
    /// 3. **The reset**, which cannot fail and therefore goes last.
    ///
    /// **Both refusals happen before any draw**, the rule
    /// [`select_mine`](GameState::select_mine) holds for the same reason: a refusal
    /// that changes nothing must not move the position of a run's dice either.
    ///
    /// ## What is *not* reset
    ///
    /// - **The generator.** Its position is run state, not a constant
    ///   ([`rng`](GameState)); rewinding it would deal the player an identical second
    ///   run — same grids, same procs, same Excavator — which is the opposite of what
    ///   re-walking the progression is for. The fresh Stone mine below is drawn from
    ///   wherever the run had got to.
    /// - **[`last_seen`](GameState).** Prestiging is neither a save nor an absence, so
    ///   the mark the next offline accrual measures from must not move.
    ///
    /// Everything else goes, including three things `docs/MECHANICS.md`'s list does
    /// not name and which would otherwise survive by omission: the boost reserve (ore
    /// already converted, plus the charges the erased levels granted), the auto-miner's
    /// carries, and the mines left behind — a run that kept its `visited` map would
    /// hold a richness-9 End grid its level-1 player is no longer allowed to enter.
    ///
    /// **No [`GameEvent`].** Events describe what happened inside a
    /// [`tick`](GameState::tick); this is a direct call whose [`Result`] is the whole
    /// answer, and the front-end that made it already knows it did.
    ///
    /// [`economy::pay`]: crate::economy
    pub fn prestige(&mut self) -> Result<(), CoreError> {
        if !self.player.has_unlocked(World::End) {
            return Err(CoreError::PrestigeLocked {
                level: self.player.get_level(),
                needed: World::End.unlock_level(),
            });
        }

        let cost = prestige::cost(self.player.get_prestige());
        economy::pay(self.player.inventory_mut(), &cost)?;

        self.player.prestige_reset();
        self.visited.clear();
        self.mine = Mine::new(MineKind::Stone, &mut self.rng);
        self.boost_charges = 0;
        self.active_boost = None;
        self.auto_common_progress = 0;
        self.auto_value_progress = 0;
        self.yield_carry.clear();
        Ok(())
    }
}

/// The running total for `item` in a `(item, amount)` list, inserted at zero if the
/// list has not seen it yet.
///
/// A free function, and a **linear scan over a [`Vec`]** rather than a
/// [`HashMap`] entry, for the reason [`yield_carry`](GameState) gives: the lists it
/// walks hold at most three items, so hashing costs more than looking. It exists at
/// all because three call sites wanted the same six lines — the auto-miner's merge of
/// two blocks that may be the same one, and both halves of the swing's haul.
///
/// The two-step `position` then index is not a detour around the borrow checker so
/// much as the shape it accepts: returning `&mut` into a `Vec` from inside the `else`
/// of a `if let Some(…) = iter_mut().find(…)` keeps the search's borrow alive across
/// the `push`, which is the classic rejection this pattern answers.
fn entry(list: &mut Vec<(Item, u32)>, item: Item) -> &mut u32 {
    let index = match list.iter().position(|(held, _)| *held == item) {
        Some(index) => index,
        None => {
            list.push((item, 0));
            list.len() - 1
        }
    };
    &mut list[index].1
}

/// What an absence produced, for the "welcome back" the player is owed.
///
/// **It shows the multiplication.** `rate × elapsed` is the entire mechanic, so
/// `organization/UI-EN.md` §5.7.4 prints both factors and the product — a number
/// the player can check in their head is a number they trust. The report therefore
/// carries `blocks` and both durations rather than only the loot.
///
/// [`counted`](OfflineReport::counted) is separate from
/// [`elapsed`](OfflineReport::elapsed) so the cap can be *stated*: "you were away
/// for 9d 4h — counted 7d" is honest, and silently paying for seven of nine reads
/// as a bug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineReport {
    /// How long the player was actually away.
    pub elapsed: Duration,
    /// How much of that was paid for: `elapsed`, or the cap when it is shorter.
    pub counted: Duration,
    /// Whether the cap bit — `counted < elapsed`, named so the UI need not compare.
    pub capped: bool,
    /// How many cells the auto-miner brought up over `counted`.
    pub blocks: u64,
    /// What it credited, one line per item, in the denomination it lands in: raw.
    pub gained: Vec<(Item, u32)>,
}

/// What the front-end feeds one [`tick`](GameState::tick).
///
/// A struct of one field rather than a bare `bool`, because the tick's inputs are
/// a set that grows: the accessibility toggle and the auto-miner's future controls
/// all arrive here, and each one added to a positional signature is a call site
/// that silently keeps compiling with the arguments in the wrong order.
///
/// Producing `space_held` is the front-end's problem and a hard one — a terminal
/// never sends a key *release* — but none of that reaches here. The core is
/// **given** the bool and never infers it, which is what keeps "same inputs, same
/// outputs" true of a mechanic that has no deterministic source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    /// Whether the player is holding the mine key this tick.
    pub space_held: bool,
}

/// Something that happened inside a [`tick`](GameState::tick) and that the player
/// is owed an announcement about.
///
/// **Not a log of everything.** The set is `docs/DECISIONS.md`'s list of mechanics
/// that need an announcement, and nothing else: an ordinary break is already
/// visible in the inventory and the progress bar, so it emits nothing. One buffer
/// serves both the 3-second toast and the Stats history, which is what makes
/// "ephemeral plus history" one feature rather than two.
///
/// The variants carry **data, never presentation**: no colours, no durations, no
/// wall-clock instants. A toast's window and a proc flash's 200 ms decay are the
/// front-end's, because they are nothing but an ambient clock — the one thing the
/// core's determinism contract keeps on the other side of the boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    /// The player reached `level`, and what it handed over.
    ///
    /// `reward` is [`None`] only at the ends of the ladder, where
    /// [`reward_for_level`](crate::reward::reward_for_level) has nothing to give.
    /// The event still fires: a level reached is worth saying even when it pays
    /// nothing.
    LevelUp {
        /// The level just reached.
        level: u32,
        /// What it granted, if anything.
        reward: Option<LevelReward>,
    },
    /// A spatial enchant fired, and these are the cells its shape covered.
    ///
    /// **`cells`, not a count**, and that is a hard requirement from `UI-EN.md`
    /// §5.9: the front-end paints the blast over the grid, and a `{ blocks: 200 }`
    /// leaves it re-deriving the geometry from the enchant level — a second copy of
    /// [`blast_cells`](crate::enchant::EnchantType) living in the wrong crate.
    ///
    /// The shape is what the blast *covered*, so it includes cells that were
    /// already holes. That is deliberate: a blast the player watches must look like
    /// a blast, not like the four cells that happened to still be standing.
    SpatialProc {
        /// Which of the three fired.
        kind: EnchantType,
        /// The impact cell it was centred on.
        origin: (u8, u8),
        /// Every grid cell its shape covered.
        cells: Vec<(u8, u8)>,
    },
    /// The Excavator substituted the impact block's drop with a Compressed unit.
    ExcavatorProc {
        /// The Compressed unit banked in the ordinary drop's place.
        item: Item,
    },
    /// The mine emptied and came back whole — the batch reset.
    MineRefilled {
        /// Which mine refilled. Carried because a run owns twelve and only one is
        /// in front of the player, so an unqualified "it refilled" is ambiguous the
        /// moment anything but the current mine can produce one.
        kind: MineKind,
    },
    /// The running boost ran out.
    BoostExpired,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::economy::CostLine;
    use crate::enchant::Enchants;
    use crate::material::Material;
    use crate::pickaxe::{Pickaxe, PickaxeTier};
    use crate::tunables::BOOST_COST;
    use crate::world::World;

    /// A state at a fixed seed and a fixed instant: every test here counts draws
    /// from the same starting position.
    fn state() -> GameState {
        GameState::new(42, SystemTime::UNIX_EPOCH)
    }

    /// The tick a held Space produces.
    const MINING: Input = Input { space_held: true };
    /// The tick a released Space produces.
    const IDLE: Input = Input { space_held: false };

    /// Installs a pickaxe on the run under test.
    ///
    /// The only way in: [`Player`]'s fields are private, and the shop is the sole
    /// public door to a stronger tool. A test that had to *buy* its way to Netherite
    /// would be a test of the economy wearing a tick's clothes.
    fn equip(state: &mut GameState, tier: PickaxeTier, enchants: Enchants) {
        let (_, pickaxe) = state.player.inventory_and_pickaxe_mut();
        *pickaxe = Pickaxe::new(tier, enchants);
    }

    /// One enchant at the highest level the End allows.
    fn only(kind: EnchantType, levels: u8) -> Enchants {
        let mut enchants = Enchants::new();
        for _ in 0..levels {
            assert!(
                enchants
                    .upgrade(kind, PickaxeTier::Netherite, World::End)
                    .is_ok()
            );
        }
        enchants
    }

    /// A power that clears any block in the game in one tick, so a swing is one
    /// call and the test is about the swing rather than about waiting for it.
    fn instamining() -> Enchants {
        only(EnchantType::Efficiency, 15)
    }

    /// How far the generator has advanced, measured by what it draws next.
    ///
    /// The position is not readable, so it is compared by *consequence*: two states
    /// whose next draws agree are at the same point in the sequence. Drawing from a
    /// clone leaves the state under test untouched.
    fn next_draws(state: &GameState) -> Vec<Block> {
        let mut rng = state.rng.clone();
        (0..8)
            .map(|_| Mine::new(MineKind::Stone, &mut rng))
            .filter_map(|m| m.get(0, 0))
            .collect()
    }

    /// The starter mine is the one the player opens on, and it is already dug-able:
    /// a full grid, not an empty shell waiting for a first selection.
    #[test]
    fn a_new_run_starts_in_a_full_stone_mine() {
        let state = state();

        assert_eq!(state.current_mine().kind(), MineKind::Stone);
        assert_eq!(
            state.current_mine().remaining_count(),
            state.current_mine().capacity()
        );
        assert_eq!(state.player().get_level(), 1);
        assert_eq!(state.boost_charges(), 0);
        assert!(state.active_boost().is_none());
    }

    /// Same seed, same run. The whole determinism contract in one assertion: the
    /// grid a run opens on is a function of the seed and nothing else.
    #[test]
    fn the_same_seed_opens_the_same_run() {
        let a = GameState::new(7, SystemTime::UNIX_EPOCH);
        let b = GameState::new(7, SystemTime::UNIX_EPOCH);

        assert_eq!(a.current_mine().get_grid(), b.current_mine().get_grid());
    }

    /// The gate the whole two-axis progression rests on, on a mine that closes
    /// **both** axes: Obsidian is in the Nether (level 15) behind a Diamond
    /// pickaxe. The refusal names each one, because either alone would send the
    /// player off to buy something that still leaves the door shut.
    #[test]
    fn a_locked_mine_is_refused_with_both_axes_named() {
        let mut state = state();
        let lock = state.player().mine_lock(MineKind::Obsidian);

        assert_eq!(
            state.select_mine(MineKind::Obsidian),
            Err(CoreError::MineLocked {
                kind: MineKind::Obsidian,
                lock,
            })
        );

        assert_eq!(lock.missing_level(), Some(15));
        assert_eq!(lock.missing_tier(), Some(PickaxeTier::Diamond));
        assert_eq!(
            CoreError::MineLocked {
                kind: MineKind::Obsidian,
                lock,
            }
            .to_string(),
            "the Obsidian mine needs level 15 and a Diamond pickaxe"
        );
    }

    /// The End's own mine locks on **one** axis only — End Stone is Wooden-gated,
    /// so what stands between a fresh player and the Amethyst is the level alone.
    /// The message must not invent a pickaxe requirement that is not there.
    #[test]
    fn a_mine_short_on_one_axis_names_only_that_one() {
        let mut state = state();
        let lock = state.player().mine_lock(MineKind::Amethyst);
        let err = CoreError::MineLocked {
            kind: MineKind::Amethyst,
            lock,
        };

        assert_eq!(state.select_mine(MineKind::Amethyst), Err(err));

        assert_eq!(lock.missing_level(), Some(30));
        assert_eq!(lock.missing_tier(), None);
        assert_eq!(err.to_string(), "the End mine needs level 30");
    }

    /// A refusal changes nothing — **including the generator**. A first visit mints
    /// a grid, so a gate checked after the draw would let a refused selection move
    /// the position of a run's dice, and every later draw with it.
    #[test]
    fn a_refused_mine_draws_nothing_and_changes_nothing() {
        let mut state = state();
        let before = next_draws(&state);

        assert!(state.select_mine(MineKind::Amethyst).is_err());

        assert_eq!(state.current_mine().kind(), MineKind::Stone);
        assert!(state.mine(MineKind::Amethyst).is_none());
        assert_eq!(next_draws(&state), before);
    }

    /// Re-affirming the mine you are in says nothing, so nothing moves. A front-end
    /// that re-selects on every keypress must not reroll the run's dice with it.
    #[test]
    fn selecting_the_mine_you_are_in_draws_nothing() {
        let mut state = state();
        let grid = state.current_mine().get_grid().to_vec();
        let before = next_draws(&state);

        assert!(state.select_mine(MineKind::Stone).is_ok());

        assert_eq!(state.current_mine().get_grid(), grid);
        assert_eq!(next_draws(&state), before);
    }

    /// *Mines persist*: leaving one and coming back finds it as it was left, holes
    /// and all. Regenerating on entry would be a free batch reset — break the four
    /// valuable cells, walk out, walk back in, break them again.
    #[test]
    fn a_mine_left_and_re_entered_keeps_its_holes() {
        let mut state = state();
        // Coal shares the Overworld and the Wooden gate, so it is open at level 1.
        equip(&mut state, PickaxeTier::Netherite, instamining());
        assert!(state.select_mine(MineKind::Coal).is_ok());
        for _ in 0..5 {
            state.tick(MINING);
        }
        let dug = state.current_mine().get_grid().to_vec();
        assert!(
            dug.iter().flatten().any(Option::is_none),
            "the test needs holes to be about anything"
        );

        assert!(state.select_mine(MineKind::Stone).is_ok());
        assert!(state.select_mine(MineKind::Coal).is_ok());

        assert_eq!(state.current_mine().get_grid(), dug.as_slice());
    }

    /// The second visit to a mine restores it rather than minting a new one, and
    /// the proof is that it costs no draw: a fresh grid would advance the generator
    /// by one cell per cell.
    #[test]
    fn returning_to_a_visited_mine_costs_no_draw() {
        let mut state = state();
        assert!(state.select_mine(MineKind::Coal).is_ok());
        assert!(state.select_mine(MineKind::Stone).is_ok());
        let before = next_draws(&state);
        let coal = state
            .mine(MineKind::Coal)
            .map(|m| m.get_grid().to_vec())
            .unwrap_or_default();

        assert!(state.select_mine(MineKind::Coal).is_ok());

        assert_eq!(next_draws(&state), before);
        assert_eq!(state.current_mine().get_grid(), coal.as_slice());
    }

    /// The mine in front of the player is reachable both ways, and they agree.
    /// `mine(kind)` must not answer `None` for the one mine that is certainly open.
    #[test]
    fn the_current_mine_is_visible_through_the_lookup_too() {
        let state = state();

        assert_eq!(
            state.mine(MineKind::Stone).map(Mine::kind),
            Some(MineKind::Stone)
        );
        assert!(state.mine(MineKind::Coal).is_none());
    }

    /// A purchase reaches the player's own inventory and pickaxe — the borrow split
    /// this module exists to arrange. Without stock it refuses, and the refusal is
    /// the same one the economy would give.
    #[test]
    fn a_purchase_spends_the_players_own_inventory() {
        let mut state = state();

        assert!(matches!(
            state.buy_pickaxe_efficiency(),
            Err(CoreError::InsufficientItems { .. })
        ));

        let cost = economy::pickaxe_efficiency_cost(PickaxeTier::Wooden, 0);
        for line in cost.lines() {
            for (item, amount) in line.requirements() {
                state.player.inventory_mut().add(item, amount);
            }
        }
        assert!(state.buy_pickaxe_efficiency().is_ok());

        assert_eq!(
            state
                .player()
                .get_pickaxe()
                .enchants()
                .get_level(EnchantType::Efficiency),
            1
        );
    }

    /// A bought charge lands in the reserve, not in a running boost: the player
    /// fires it when the mine in front of them is worth it.
    #[test]
    fn a_bought_boost_lands_in_the_reserve_unlit() {
        let mut state = state();
        state.player.inventory_mut().add(
            Item::Compressed(Material::Redstone),
            BOOST_COST / crate::tunables::RAW_PER_COMPRESSED,
        );

        assert!(state.buy_boost_charge().is_ok());

        assert_eq!(state.boost_charges(), 1);
        assert!(state.active_boost().is_none());
    }

    /// Every forwarding door refuses for the reason its own track would give,
    /// which is the only thing these wrappers can get wrong: a borrow arranged
    /// correctly but handed to the wrong function would still compile and still
    /// refuse — just with somebody else's error.
    #[test]
    fn each_door_forwards_to_the_track_that_owns_the_rule() {
        let mut state = state();

        assert!(matches!(
            state.buy_pickaxe_tier(),
            Err(CoreError::EfficiencyNotMaxed { current: 0, .. })
        ));
        assert!(matches!(
            state.buy_enchant(EnchantType::Fortune),
            Err(CoreError::InsufficientItems { .. })
        ));
        assert!(matches!(
            state.buy_mine_size(),
            Err(CoreError::InsufficientItems { .. })
        ));
        assert!(matches!(
            state.buy_mine_richness(),
            Err(CoreError::InsufficientItems { .. })
        ));
        // The dial is free, so what refuses it is the ceiling, not the till: a
        // fresh mine has bought no richness level, so every setting above 0 is
        // above the ceiling.
        assert_eq!(
            state.set_richness_setting(1),
            Err(CoreError::RichnessAboveCeiling {
                requested: 1,
                ceiling: 0,
            })
        );
    }

    /// The three-way borrow, proven at runtime rather than only at compile time:
    /// the inventory is debited, the mine grows, and the grid it redraws comes from
    /// this run's generator — three fields of one `&mut self`, reached separately.
    #[test]
    fn buying_a_size_level_spends_the_inventory_and_grows_the_grid() {
        let mut state = state();
        let before = state.current_mine().capacity();
        let cost = economy::mine_size_cost(MineKind::Stone, 0);
        for line in cost.lines() {
            for (item, amount) in line.requirements() {
                state.player.inventory_mut().add(item, amount);
            }
        }

        assert!(state.buy_mine_size().is_ok());

        assert_eq!(state.current_mine().get_size_level(), 1);
        assert!(state.current_mine().capacity() > before);
        assert_eq!(
            state.current_mine().remaining_count(),
            state.current_mine().capacity(),
            "an enlargement redraws a full grid"
        );
        assert_eq!(
            state.player().get_inventory().raw_value(Material::Stone),
            0,
            "the cost came out of the player's own inventory"
        );
    }

    /// A mine that is level-open but tier-shut names only the pickaxe. The Iron
    /// mine is in the Overworld, so a level-1 player is already past its world
    /// gate — what stops them is a Wooden pickaxe against a Stone requirement.
    #[test]
    fn a_tier_locked_mine_names_only_the_pickaxe() {
        let mut state = state();
        let lock = state.player().mine_lock(MineKind::Iron);
        let err = CoreError::MineLocked {
            kind: MineKind::Iron,
            lock,
        };

        assert_eq!(state.select_mine(MineKind::Iron), Err(err));

        assert_eq!(lock.missing_level(), None);
        assert_eq!(lock.missing_tier(), Some(PickaxeTier::Stone));
        assert_eq!(err.to_string(), "the Iron mine needs a Stone pickaxe");
    }

    /// A refused purchase leaves the reserve alone. The increment is this struct's
    /// half of the transaction, and it must not run when the debit did not.
    #[test]
    fn a_refused_boost_purchase_grants_no_charge() {
        let mut state = state();

        assert!(state.buy_boost_charge().is_err());

        assert_eq!(state.boost_charges(), 0);
    }

    /// The determinism contract, asserted where it can actually be broken: a state
    /// built at one instant and one built at another are the same run. Nothing in
    /// core may consult the clock it is handed, only store it.
    #[test]
    fn a_state_reads_no_clock_of_its_own() {
        let epoch = GameState::new(3, SystemTime::UNIX_EPOCH);
        let later = GameState::new(
            3,
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(9_000_000),
        );

        assert_eq!(
            epoch.current_mine().get_grid(),
            later.current_mine().get_grid()
        );
        assert_ne!(epoch.last_seen(), later.last_seen());
    }

    // --- The swing ---

    /// A released Space is not a slow swing, it is **no** swing: no cell falls, no
    /// experience is granted, and — the part that matters beyond the obvious — the
    /// generator does not move. A tick that drew even once while idle would make a
    /// run's dice depend on how long the player sat in a menu.
    ///
    /// The auto-miner keeps running, and that is a separate system: it takes no
    /// cell and draws nothing, so it disturbs neither half of this.
    #[test]
    fn a_released_space_bar_swings_at_nothing_and_draws_nothing() {
        let mut state = state();
        equip(&mut state, PickaxeTier::Netherite, instamining());
        let before = next_draws(&state);
        let full = state.current_mine().remaining_count();

        for _ in 0..100 {
            assert!(state.tick(IDLE).is_empty());
        }

        assert_eq!(state.current_mine().remaining_count(), full, "a cell fell");
        assert_eq!(state.player().get_experience(), 0, "idling bought a level");
        assert_eq!(state.current_mine().break_ratio(), 0.0, "progress was made");
        assert_eq!(next_draws(&state), before, "an idle tick drew from the rng");
    }

    /// The ordinary swing: one cell falls, its drop is banked, its experience is
    /// granted, and **no event is emitted**. Announcing every break would allocate
    /// on every tick the player holds Space to say what the inventory already says.
    #[test]
    fn an_ordinary_break_banks_its_drop_and_announces_nothing() {
        let mut state = state();
        equip(&mut state, PickaxeTier::Netherite, instamining());
        let full = state.current_mine().remaining_count();

        let events = state.tick(MINING);

        assert!(events.is_empty(), "an ordinary break announced {events:?}");
        assert_eq!(state.current_mine().remaining_count(), full - 1);
        assert!(state.player().get_inventory().raw_value(Material::Stone) > 0);
        assert!(state.player().get_experience() > 0);
    }

    /// A block that takes many ticks pays on the tick it gives, and on no earlier
    /// one. Progressive breaking, seen from the tick rather than from the mine.
    #[test]
    fn a_hard_block_pays_only_on_the_tick_it_gives() {
        let mut state = state();
        assert!(state.select_mine(MineKind::Coal).is_ok());
        let held = state.player().get_inventory().raw_value(Material::Coal);

        // A bare Wooden pickaxe against Coal: many ticks, and none of them pay.
        for _ in 0..5 {
            assert!(state.tick(MINING).is_empty());
            assert_eq!(
                state.player().get_inventory().raw_value(Material::Coal),
                held,
                "a partial swing paid out"
            );
        }
        assert!(state.current_mine().break_ratio() > 0.0, "no progress made");

        while state.player().get_inventory().raw_value(Material::Coal) == held {
            state.tick(MINING);
        }
        assert_eq!(state.current_mine().break_ratio(), 0.0, "progress carried");
    }

    /// **The swing's order, proven at its hardest point.** A maxed Nuke can clear
    /// the whole grid on one swing; the mine must still come back full at the end of
    /// that same tick, and every cell it took must have been paid for. A refill
    /// fired mid-swing would instead hand the enchants that have not rolled yet a
    /// fresh two hundred cells to blast.
    #[test]
    fn a_swing_pays_every_cell_it_takes_before_the_mine_comes_back() {
        let mut state = state();
        let mut enchants = instamining();
        for _ in 0..10 {
            assert!(
                enchants
                    .upgrade(EnchantType::Nuke, PickaxeTier::Netherite, World::End)
                    .is_ok()
            );
        }
        equip(&mut state, PickaxeTier::Netherite, enchants);
        let capacity = state.current_mine().capacity();

        // Swing until a Nuke lands; its proc rate is low by design.
        let mut refills = 0;
        let mut banked_before_refill = 0;
        for _ in 0..2_000 {
            let held = state.player().get_inventory().raw_value(Material::Stone);
            let events = state.tick(MINING);
            if events
                .iter()
                .any(|e| matches!(e, GameEvent::MineRefilled { .. }))
            {
                refills += 1;
                banked_before_refill =
                    state.player().get_inventory().raw_value(Material::Stone) - held;
                break;
            }
        }

        assert_eq!(refills, 1, "no swing ever emptied the grid in 2 000 ticks");
        assert!(
            banked_before_refill > 1,
            "the refilling swing paid for one cell, so the blast went unpaid"
        );
        assert_eq!(
            state.current_mine().remaining_count(),
            capacity,
            "the mine did not come back at the end of the swing"
        );
    }

    /// A spatial proc announces **which cells it covered**, not how many blocks it
    /// broke. The front-end paints the shape, so a count would leave it re-deriving
    /// the geometry — and the shape includes ground already dug, because a blast the
    /// player watches must look like a blast.
    #[test]
    fn a_spatial_proc_announces_the_shape_it_covered() {
        let mut state = state();
        let mut enchants = instamining();
        for _ in 0..10 {
            assert!(
                enchants
                    .upgrade(EnchantType::Explosive, PickaxeTier::Netherite, World::End)
                    .is_ok()
            );
        }
        equip(&mut state, PickaxeTier::Netherite, enchants);

        let mut seen = None;
        for _ in 0..2_000 {
            if let Some(event) = state
                .tick(MINING)
                .into_iter()
                .find(|e| matches!(e, GameEvent::SpatialProc { .. }))
            {
                seen = Some(event);
                break;
            }
        }

        let Some(GameEvent::SpatialProc {
            kind,
            origin,
            cells,
        }) = seen
        else {
            unreachable!("a maxed Explosive never fired in 2 000 swings")
        };
        assert_eq!(kind, EnchantType::Explosive);
        assert!(cells.contains(&origin), "the shape missed its own centre");
        let (width, height) = state.current_mine().get_size();
        assert!(
            cells.iter().all(|&(x, y)| x < width && y < height),
            "a cell of the shape is off the grid"
        );
    }

    /// **Fortune multiplies the loot and never the experience.** The rule is already
    /// unwritable one level down — `grant_break_experience` takes no pickaxe — so
    /// this pins it *here*, in the one place both are in scope and a future edit
    /// could wire them together.
    #[test]
    fn fortune_multiplies_the_loot_and_never_the_experience() {
        fn run(fortune: u8) -> (u32, u32) {
            let mut state = GameState::new(11, SystemTime::UNIX_EPOCH);
            let mut enchants = instamining();
            for _ in 0..fortune {
                assert!(
                    enchants
                        .upgrade(EnchantType::Fortune, PickaxeTier::Netherite, World::End)
                        .is_ok()
                );
            }
            equip(&mut state, PickaxeTier::Netherite, enchants);
            for _ in 0..30 {
                state.tick(MINING);
            }
            (
                state.player().get_inventory().raw_value(Material::Stone),
                state.player().get_experience(),
            )
        }

        let (plain_loot, plain_xp) = run(0);
        let (rich_loot, rich_xp) = run(10);

        assert_eq!(rich_loot, plain_loot * 11, "Fortune 10 is eleven times");
        assert_eq!(rich_xp, plain_xp, "Fortune reached the experience");
    }

    /// The Excavator **replaces** the impact block's drop with a Compressed unit and
    /// takes no Fortune with it. The two rarest levers in the game are deliberately
    /// kept from composing, so a proc is worth exactly one Compressed unit however
    /// fortunate the pickaxe.
    #[test]
    fn an_excavator_proc_replaces_the_impact_drop_and_takes_no_fortune() {
        let mut state = state();
        let mut enchants = instamining();
        for _ in 0..10 {
            for kind in [EnchantType::Excavator, EnchantType::Fortune] {
                assert!(
                    enchants
                        .upgrade(kind, PickaxeTier::Netherite, World::End)
                        .is_ok()
                );
            }
        }
        equip(&mut state, PickaxeTier::Netherite, enchants);

        let mut fired = None;
        for _ in 0..2_000 {
            let compressed = state
                .player()
                .get_inventory()
                .count(Item::Compressed(Material::Stone));
            let raw = state.player().get_inventory().raw_value(Material::Stone);
            let events = state.tick(MINING);
            if let Some(event) = events
                .iter()
                .find(|e| matches!(e, GameEvent::ExcavatorProc { .. }))
            {
                let gained_compressed = state
                    .player()
                    .get_inventory()
                    .count(Item::Compressed(Material::Stone))
                    - compressed;
                let gained_raw = state.player().get_inventory().raw_value(Material::Stone) - raw;
                fired = Some((event.clone(), gained_compressed, gained_raw));
                break;
            }
        }

        let Some((event, compressed, raw)) = fired else {
            unreachable!("a maxed Excavator never fired in 2 000 swings")
        };
        assert_eq!(
            event,
            GameEvent::ExcavatorProc {
                item: Item::Compressed(Material::Stone),
            }
        );
        assert_eq!(compressed, 1, "a proc is worth exactly one Compressed unit");
        // `raw_value` counts the Compressed unit too, so what the impact block would
        // have paid — 1 raw before Fortune, 11 after — has to be absent from it.
        assert_eq!(
            raw,
            crate::tunables::RAW_PER_COMPRESSED,
            "the impact block paid its ordinary drop on top of the substitution"
        );
    }

    /// Crossing a level announces it and hands over what the level owes: the ore
    /// bundle lands in the inventory, raw, and a charge lands in the reserve.
    #[test]
    fn crossing_a_level_announces_it_and_pays_what_it_owes() {
        let mut state = state();
        equip(&mut state, PickaxeTier::Netherite, instamining());

        let mut levels = Vec::new();
        for _ in 0..600 {
            for event in state.tick(MINING) {
                if let GameEvent::LevelUp { level, reward } = event {
                    levels.push((level, reward));
                }
            }
        }

        let Some((level, reward)) = levels.first() else {
            unreachable!("600 instamined Stone cells bought no level")
        };
        assert_eq!(*level, 2, "the first level crossed must be the second");
        assert_eq!(state.player().get_level() as usize, 1 + levels.len());

        let Some(reward) = reward else {
            unreachable!("level 2 owes a reward")
        };
        let Payout::Ore(lines) = &reward.payout else {
            unreachable!("level 2 pays ore, not a world")
        };
        for &(item, amount) in lines {
            assert!(matches!(item, Item::Raw(_)), "a bundle was pre-compressed");
            assert!(
                state.player().get_inventory().count(item) >= amount,
                "the bundle never landed"
            );
        }
    }

    // --- Boosts ---

    /// A boost multiplies the mining power while it runs, and the tick is where the
    /// two halves of the haste product finally meet: the permanent enchant on the
    /// pickaxe, and the temporary boost on the run.
    #[test]
    fn a_running_boost_multiplies_the_power_the_swing_uses() {
        let mut state = state();
        equip(&mut state, PickaxeTier::Wooden, Enchants::new());
        let bare = state.player().get_pickaxe().mining_power();

        state.active_boost = Some(Boost::new(BOOST_MULTIPLIER, BOOST_DURATION_TICKS));

        assert_eq!(state.boost_multiplier(), BOOST_MULTIPLIER);
        let boosted = bare * state.boost_multiplier();
        assert!(boosted > bare, "the boost did not speed anything up");
    }

    /// Firing a charge lights a boost and spends the charge. The reserve is what the
    /// player holds; the running boost is what they are spending.
    #[test]
    fn firing_a_charge_lights_a_boost_and_spends_it() {
        let mut state = state();
        state.boost_charges = 2;

        assert!(state.fire_boost().is_ok());

        assert_eq!(state.boost_charges(), 1);
        assert_eq!(
            state.active_boost().map(Boost::remaining_ticks),
            Some(BOOST_DURATION_TICKS)
        );
    }

    /// An empty reserve refuses, and — the half that matters — **changes nothing**.
    /// A decrement on the failing path would underflow a `u32` straight to 4.29
    /// billion charges.
    #[test]
    fn firing_with_an_empty_reserve_is_refused_and_changes_nothing() {
        let mut state = state();

        assert_eq!(state.fire_boost(), Err(CoreError::NoBoostCharge));

        assert_eq!(state.boost_charges(), 0);
        assert!(state.active_boost().is_none());
        assert_eq!(
            CoreError::NoBoostCharge.to_string(),
            "no boost charge to fire"
        );
    }

    /// A charge fired onto a running boost **stacks**: the core allows it and adds
    /// the time, leaving the "are you sure?" to the interface, which can see the
    /// running boost. Refusing here would put that question beyond every front-end.
    #[test]
    fn a_charge_fired_onto_a_running_boost_stacks_rather_than_restarting_it() {
        let mut state = state();
        state.boost_charges = 2;
        assert!(state.fire_boost().is_ok());
        state.tick(IDLE);

        assert!(state.fire_boost().is_ok());

        assert_eq!(state.boost_charges(), 0);
        assert_eq!(
            state.active_boost().map(Boost::remaining_ticks),
            Some(2 * BOOST_DURATION_TICKS - 1),
            "the second charge restarted the clock instead of adding to it"
        );
    }

    /// The reserve outlives the boost it did not light. A charge bought before a
    /// session and never fired must still be there after another one has run its
    /// course — the whole reason the reserve is a field and not a collection of
    /// [`Boost`]s counting themselves down.
    #[test]
    fn the_reserve_survives_a_boost_running_out() {
        let mut state = state();
        state.boost_charges = 3;
        assert!(state.fire_boost().is_ok());

        for _ in 0..BOOST_DURATION_TICKS {
            state.tick(IDLE);
        }

        assert!(state.active_boost().is_none(), "the boost never lapsed");
        assert_eq!(
            state.boost_charges(),
            2,
            "the reserve burned down unwatched"
        );
    }

    /// A boost lapses on the tick that empties it, and says so exactly once. A lapse
    /// the player is not told about is a mining rate that silently drops by more
    /// than half.
    #[test]
    fn a_lapsing_boost_is_announced_once() {
        let mut state = state();
        state.active_boost = Some(Boost::new(BOOST_MULTIPLIER, 3));

        assert!(state.tick(IDLE).is_empty());
        assert!(state.tick(IDLE).is_empty());
        assert_eq!(state.tick(IDLE), vec![GameEvent::BoostExpired]);

        assert!(state.active_boost().is_none());
        assert_eq!(state.boost_multiplier(), 1.0, "an expired boost still ran");
        assert!(state.tick(IDLE).is_empty(), "the lapse was announced twice");
    }

    // --- The auto-miner ---

    /// A flat rate, credited even while the player mines by hand, and it lands in
    /// the same inventory. The rate is below one block per tick, so the first few
    /// ticks pay nothing at all — the carry is what makes them pay eventually.
    #[test]
    fn the_auto_miner_pays_a_flat_rate_while_the_player_does_nothing() {
        let mut state = state();

        let mut ticks = 0;
        while state.player().get_inventory().raw_value(Material::Stone) == 0 {
            state.tick(IDLE);
            ticks += 1;
            assert!(ticks < 1_000, "the carry never added up to a whole cell");
        }

        // The rate is under one block a tick, so the first cell has to take more
        // than one — otherwise the fraction is being paid out whole, which over an
        // absence would multiply the auto-miner's output by the reciprocal of its
        // own rate.
        let ticks_per_block = MILLIBLOCKS_PER_BLOCK / AUTO_MINER_MILLIBLOCKS_PER_TICK;
        assert!(
            ticks >= ticks_per_block,
            "a cell arrived in {ticks} ticks, sooner than the rate allows"
        );
    }

    /// **The identity the closed form claims**: crediting a span in one call and
    /// crediting it tick by tick pay exactly the same. This is what makes the
    /// offline path legitimate — it is the online path with a bigger multiplier, and
    /// integer milliblocks are what make "exactly" literal rather than approximate.
    #[test]
    fn a_span_credited_at_once_pays_what_the_same_span_pays_tick_by_tick() {
        const SPAN: u64 = 4_321;

        let mut piecemeal = state();
        for _ in 0..SPAN {
            piecemeal.tick(IDLE);
        }

        let mut lump = state();
        lump.credit_auto_mining(SPAN);

        assert_eq!(
            lump.player().get_inventory().raw_value(Material::Stone),
            piecemeal
                .player()
                .get_inventory()
                .raw_value(Material::Stone)
        );
        assert_eq!(lump.auto_common_progress, piecemeal.auto_common_progress);
        assert_eq!(lump.auto_value_progress, piecemeal.auto_value_progress);
    }

    /// **The auto-miner touches nothing it must not touch**: not the grid, not the
    /// generator, and not the level bar. Each of the three is a separate rule and
    /// each would be a separate bug — a grid it dug would empty a mine nobody
    /// watched, a draw would make a run's dice depend on how long it idled, and
    /// experience would open the Nether and the End over a long absence.
    #[test]
    fn the_auto_miner_takes_no_cell_no_draw_and_no_experience() {
        let mut state = state();
        let grid = state.current_mine().get_grid().to_vec();
        let draws = next_draws(&state);

        let gained = state.credit_auto_mining(20 * 60 * 60);

        assert!(!gained.is_empty(), "an hour of idling paid nothing");
        assert_eq!(state.current_mine().get_grid(), grid.as_slice());
        assert_eq!(next_draws(&state), draws);
        assert_eq!(state.player().get_level(), 1);
        assert_eq!(state.player().get_experience(), 0);
    }

    /// The dial the player set is what the auto-miner is weighted by: enriching a
    /// mine shifts the closed-form payout toward the value cell, exactly as it
    /// shifts the cells a hand-mined grid is drawn from.
    #[test]
    fn enriching_the_mine_shifts_what_the_auto_miner_credits() {
        fn credited(setting: u32) -> (u32, u32) {
            let mut state = state();
            // The Quartz mine is the two-material one open at level 1: Netherrack
            // common, Quartz Ore of value, so the two lines are distinguishable.
            state.mine = Mine::new(MineKind::Quartz, &mut state.rng);
            for _ in 0..setting {
                assert!(state.mine.upgrade_richness_level().is_ok());
            }
            assert!(state.set_richness_setting(setting).is_ok());

            state.credit_auto_mining(20 * 60 * 60);
            let inventory = state.player().get_inventory();
            (
                inventory.raw_value(Material::Netherrack),
                inventory.raw_value(Material::Quartz),
            )
        }

        let (poor_common, poor_value) = credited(0);
        let (rich_common, rich_value) = credited(9);

        assert!(
            rich_value > poor_value,
            "the dial did not enrich the payout"
        );
        assert!(rich_common < poor_common, "the common share did not shrink");
        assert!(
            poor_value > 0,
            "the dial at 0 starved the value cell entirely"
        );
    }

    /// A same-material mine credits **one** line, not the same item listed twice.
    /// The inventory would not care; the offline summary that renders this would
    /// print the same row back to back.
    #[test]
    fn a_same_material_mine_credits_one_line() {
        let mut state = state();

        let gained = state.credit_auto_mining(20 * 60 * 60);

        assert_eq!(gained.len(), 1, "the Stone mine credited {gained:?}");
        assert_eq!(gained[0].0, Item::Raw(Material::Stone));
    }

    /// Fortune reaches the auto-miner's loot, because it is loot. It reaches its
    /// experience nowhere, because there is none — the axis this system stays off.
    #[test]
    fn fortune_multiplies_what_the_auto_miner_brings_up() {
        fn credited(fortune: u8) -> u32 {
            let mut state = state();
            equip(
                &mut state,
                PickaxeTier::Netherite,
                only(EnchantType::Fortune, fortune),
            );
            state.credit_auto_mining(20 * 60 * 60);
            state.player().get_inventory().raw_value(Material::Stone)
        }

        assert_eq!(credited(10), credited(0) * 11);
    }

    // --- Offline accrual ---

    /// An absence is credited on resume, and the mark moves so the next one is
    /// measured from here. The report shows the multiplication the player is meant
    /// to be able to check: the span, and what it produced.
    #[test]
    fn an_absence_is_credited_and_the_mark_moves() {
        let mut state = state();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_600);

        let Some(report) = state.resume(now) else {
            unreachable!("an hour away credited nothing")
        };

        assert_eq!(report.elapsed, Duration::from_secs(3_600));
        assert_eq!(report.counted, report.elapsed);
        assert!(!report.capped);
        assert_eq!(
            report.blocks,
            3_600 * TICKS_PER_SECOND * AUTO_MINER_MILLIBLOCKS_PER_TICK / MILLIBLOCKS_PER_BLOCK
        );
        assert!(!report.gained.is_empty(), "the report credited nothing");
        assert_eq!(state.last_seen(), now, "the mark did not move");
    }

    /// **The identity the closed form rests on**: an absence pays exactly what the
    /// same span of ticks pays. If these two ever diverge, "a multiplication, not a
    /// replay" has stopped being a description of the same rule and become two.
    #[test]
    fn an_absence_pays_what_the_same_span_of_ticks_pays() {
        const SECONDS: u64 = 900;

        let mut offline = state();
        assert!(
            offline
                .resume(SystemTime::UNIX_EPOCH + Duration::from_secs(SECONDS))
                .is_some()
        );

        let mut online = state();
        for _ in 0..SECONDS * TICKS_PER_SECOND {
            online.tick(IDLE);
        }

        assert_eq!(
            offline.player().get_inventory().raw_value(Material::Stone),
            online.player().get_inventory().raw_value(Material::Stone)
        );
    }

    /// A clock that went backwards yields [`None`] and changes nothing. DST, a
    /// timezone change and an NTP correction all produce one and none is cheating,
    /// so there is no penalty, no flag — and no modal, which is what [`None`] tells
    /// the front-end.
    #[test]
    fn a_backward_clock_credits_nothing_and_says_nothing() {
        let mut state = GameState::new(42, SystemTime::UNIX_EPOCH + Duration::from_secs(10_000));
        let mark = state.last_seen();

        let report = state.resume(SystemTime::UNIX_EPOCH + Duration::from_secs(9_000));

        assert_eq!(report, None);
        assert_eq!(state.player().get_inventory().raw_value(Material::Stone), 0);
        assert_eq!(state.last_seen(), mark, "a backward clock moved the mark");
    }

    /// Resuming a run that was written a moment ago says nothing rather than
    /// announcing a zero. The condition is `elapsed > 0`, not "we relaunched".
    #[test]
    fn resuming_with_no_time_elapsed_says_nothing() {
        let mut state = state();

        assert_eq!(state.resume(SystemTime::UNIX_EPOCH), None);
    }

    /// A very long absence — or a clock set far forward — is paid at the cap, and
    /// the report **says so**. Paying seven of nine days in silence reads as a bug.
    #[test]
    fn a_long_absence_is_paid_at_the_cap_and_says_so() {
        let mut capped = state();
        let nine_days = Duration::from_secs(9 * 24 * 60 * 60);

        let Some(report) = capped.resume(SystemTime::UNIX_EPOCH + nine_days) else {
            unreachable!("nine days away credited nothing")
        };

        assert!(report.capped);
        assert_eq!(report.elapsed, nine_days);
        assert_eq!(report.counted, OFFLINE_CAP);

        // And the cap is a ceiling on the payout, not merely on the number printed.
        let mut exactly = state();
        assert!(
            exactly
                .resume(SystemTime::UNIX_EPOCH + OFFLINE_CAP)
                .is_some()
        );
        assert_eq!(
            capped.player().get_inventory().raw_value(Material::Stone),
            exactly.player().get_inventory().raw_value(Material::Stone)
        );
    }

    /// An absence grants no experience and draws no randomness — the two rules the
    /// auto-miner carries, asserted on the path where they matter most: seven days
    /// of experience would open the Nether and the End to a player who never mined.
    #[test]
    fn an_absence_opens_no_world_and_moves_no_dice() {
        let mut state = state();
        let draws = next_draws(&state);

        assert!(state.resume(SystemTime::UNIX_EPOCH + OFFLINE_CAP).is_some());

        assert_eq!(state.player().get_level(), 1, "an absence bought a level");
        assert_eq!(state.player().get_experience(), 0);
        assert_eq!(state.player().highest_unlocked_world(), World::Overworld);
        assert_eq!(next_draws(&state), draws, "an absence moved the rng");
    }

    /// `touch` moves the mark without paying: an autosave mid-session must not
    /// credit the seconds the tick loop is already crediting, and must still leave
    /// the next absence measured from itself.
    #[test]
    fn touching_the_mark_moves_it_without_paying() {
        let mut state = state();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_600);

        state.touch(now);

        assert_eq!(state.last_seen(), now);
        assert_eq!(
            state.player().get_inventory().raw_value(Material::Stone),
            0,
            "an autosave paid an offline bonus"
        );
        assert_eq!(state.resume(now), None, "the mark was not where it was set");
    }

    // --- Determinism ---

    /// **The golden vector, end to end.** A fixed seed and a fixed sequence of
    /// inputs produce one exact run — the property every other guarantee in this
    /// crate is built to serve, and the hook phase 10 will hang its balance
    /// simulations on. If this fails, the question is not "what are the new
    /// numbers?" but "what did we just do to every existing save?".
    #[test]
    fn a_seeded_run_replays_exactly() {
        fn run(seed: u64) -> (u32, u32, u32, usize) {
            let mut state = GameState::new(seed, SystemTime::UNIX_EPOCH);
            let mut enchants = instamining();
            for kind in [EnchantType::Explosive, EnchantType::Excavator] {
                for _ in 0..10 {
                    assert!(
                        enchants
                            .upgrade(kind, PickaxeTier::Netherite, World::End)
                            .is_ok()
                    );
                }
            }
            equip(&mut state, PickaxeTier::Netherite, enchants);

            let mut events = 0;
            for tick in 0..500 {
                // A duty cycle rather than a held key: the released ticks are what
                // prove an idle tick really is inert in the sequence.
                events += state
                    .tick(Input {
                        space_held: tick % 4 != 3,
                    })
                    .len();
            }
            (
                state.player().get_level(),
                state.player().get_experience(),
                state.player().get_inventory().raw_value(Material::Stone),
                events,
            )
        }

        assert_eq!(run(2024), run(2024), "the same seed diverged");
        assert_ne!(run(2024), run(1), "two seeds produced the same run");
    }

    // --- Prestige ---

    /// Puts a run one call away from a prestige: past the End's unlock level, holding
    /// exactly the Amethyst the next rank costs.
    ///
    /// It levels the player by *breaking blocks* rather than by writing the field,
    /// because it cannot do otherwise — `Player`'s fields are private to its module —
    /// and that is the better test anyway: the gate this exercises is the one a real
    /// run walks through.
    fn ready_to_prestige(state: &mut GameState) {
        state.player.grant_break_experience(&[Block::Amethyst; 700]);
        assert!(
            state.player.has_unlocked(World::End),
            "the fixture must clear the level gate for the test to be about the price"
        );

        let cost = prestige::cost(state.player.get_prestige());
        for (item, amount) in cost.lines().iter().flat_map(CostLine::requirements) {
            state.player.inventory_mut().add(item, amount);
        }
    }

    /// The level half of the condition, and the half `docs/UI.md` §6.8 says the
    /// preview leads with: Amethyst only drops in the End, so a player short of it is
    /// told how far off the *level* is, not how much ore they lack.
    #[test]
    fn a_prestige_before_the_end_is_refused_and_changes_nothing() {
        let mut state = state();
        let draws = next_draws(&state);

        assert_eq!(
            state.prestige(),
            Err(CoreError::PrestigeLocked {
                level: 1,
                needed: World::End.unlock_level(),
            })
        );

        assert_eq!(state.player().get_prestige(), 0);
        assert_eq!(state.player().get_level(), 1);
        assert_eq!(next_draws(&state), draws, "a refusal moved the dice");
    }

    /// The price half, refused by the same two-pass till every purchase uses — so an
    /// unaffordable prestige leaves the Amethyst where it was rather than taking what
    /// it can and failing.
    #[test]
    fn a_prestige_without_the_amethyst_is_refused_and_takes_nothing() {
        let mut state = state();
        ready_to_prestige(&mut state);
        // One raw Amethyst short of the price.
        assert!(
            state
                .player
                .inventory_mut()
                .remove(Item::Raw(Material::Amethyst), 1)
                .is_ok()
        );
        let held = state.player().get_inventory().raw_value(Material::Amethyst);
        let draws = next_draws(&state);

        assert!(matches!(
            state.prestige(),
            Err(CoreError::InsufficientItems { .. })
        ));

        assert_eq!(state.player().get_prestige(), 0);
        assert_eq!(
            state.player().get_inventory().raw_value(Material::Amethyst),
            held,
            "a refused prestige debited part of the price"
        );
        assert_eq!(next_draws(&state), draws, "a refusal moved the dice");
    }

    /// The trade, whole. Everything the run bought goes; the rank stays.
    #[test]
    fn a_prestige_banks_the_rank_and_resets_the_run() {
        let mut state = state();
        ready_to_prestige(&mut state);

        // A run with something to lose on every axis the reset names.
        equip(&mut state, PickaxeTier::Netherite, instamining());
        assert!(state.select_mine(MineKind::Coal).is_ok());
        assert!(state.select_mine(MineKind::Stone).is_ok());
        state.boost_charges = 3;
        state.auto_common_progress = 12_345;

        assert_eq!(state.prestige(), Ok(()));

        assert_eq!(state.player().get_prestige(), 1);
        assert_eq!(state.player().get_level(), 1);
        assert_eq!(state.player().get_pickaxe().get_tier(), PickaxeTier::Wooden);
        assert_eq!(
            state.player().get_inventory().raw_value(Material::Amethyst),
            0
        );
        assert_eq!(state.boost_charges(), 0);
        assert!(state.active_boost().is_none());
        assert_eq!(state.auto_common_progress, 0);
        assert_eq!(state.auto_value_progress, 0);
        assert!(state.yield_carry.is_empty());
        // The mines left behind go with the rest: a level-1 player must not be
        // holding a grid their level no longer opens.
        assert!(state.mine(MineKind::Coal).is_none());
        assert_eq!(state.current_mine().kind(), MineKind::Stone);
        assert_eq!(
            state.current_mine().remaining_count(),
            state.current_mine().capacity()
        );
    }

    /// **The generator is not rewound**, and this is the guarantee that separates a
    /// prestige from starting the game over: a rank that reset the dice would deal the
    /// player the identical run back, same grids and same procs, which is the opposite
    /// of what re-walking the progression is for.
    #[test]
    fn a_prestige_does_not_rewind_the_dice() {
        let mut state = state();
        ready_to_prestige(&mut state);
        assert_eq!(state.prestige(), Ok(()));

        let fresh = GameState::new(42, SystemTime::UNIX_EPOCH);
        assert_ne!(
            state.current_mine().get_grid(),
            fresh.current_mine().get_grid(),
            "the prestige dealt the opening grid a second time"
        );
    }

    /// Prestiging is neither a save nor an absence, so the mark the next offline
    /// accrual measures from must not move — a prestige that touched it would pay the
    /// player for the seconds it took them to press the key.
    #[test]
    fn a_prestige_does_not_move_the_offline_mark() {
        let mut state = state();
        let mark = state.last_seen();
        ready_to_prestige(&mut state);
        assert_eq!(state.prestige(), Ok(()));

        assert_eq!(state.last_seen(), mark);
    }

    /// The loot multiplier, end to end and at the size it actually bites: a Stone cell
    /// drops **one**, so at rank I five swings must pay six. Truncating each swing
    /// would pay five, and the rank the player just spent a run on would be worth
    /// nothing for the whole climb back.
    #[test]
    fn a_rank_pays_the_sixth_ore_five_swings_in() {
        let mut state = state();
        ready_to_prestige(&mut state);
        assert_eq!(state.prestige(), Ok(()));
        equip(&mut state, PickaxeTier::Netherite, instamining());

        for _ in 0..5 {
            state.tick(MINING);
        }

        // Five swings, five cells, one ore each — and the auto-miner is far too slow
        // to have added a sixth over five ticks.
        assert_eq!(state.player().get_inventory().raw_value(Material::Stone), 6);
    }

    /// The rank reaches the auto-miner through its **rate**, exactly once. Twice — as
    /// speed and again as yield — would compound to `×1.44` at rank I and make an
    /// absence the best use of a rank the player bought with a run.
    ///
    /// **Measured on the progress carries rather than on the inventory**, and that is
    /// the point rather than a convenience. A span long enough to credit whole cells
    /// splits into two streams that each floor and keep their own remainder, so the
    /// ore banked lands *near* the exact product and not on it — the carries are where
    /// the missing fraction went. A span too short to credit anything leaves the whole
    /// scaled rate sitting in those carries, where it can be compared exactly.
    #[test]
    fn a_rank_multiplies_the_auto_miners_rate_once() {
        let mut unranked = state();
        unranked.credit_auto_mining(1);

        let mut ranked = state();
        ready_to_prestige(&mut ranked);
        assert_eq!(ranked.prestige(), Ok(()));
        ranked.credit_auto_mining(1);

        let permille = u64::from(prestige::multiplier_permille(1));
        let denominator = u64::from(prestige::PERMILLE);
        for (plain, boosted) in [
            (unranked.auto_common_progress, ranked.auto_common_progress),
            (unranked.auto_value_progress, ranked.auto_value_progress),
        ] {
            assert!(plain > 0, "the tick has to have produced something");
            assert_eq!(
                boosted * denominator,
                plain * permille,
                "rank I moved the rate to {boosted} where it was {plain}"
            );
        }
    }

    /// The offline identity, re-run **at a rank**: the closed form is only legitimate
    /// while crediting a span in one call equals crediting it tick by tick, and a
    /// prestige multiplier applied to a truncated rate would have broken exactly that.
    /// It divides last for this reason.
    #[test]
    fn a_ranked_span_credited_at_once_still_pays_what_it_pays_tick_by_tick() {
        const SPAN: u64 = 4_321;

        let mut piecemeal = state();
        ready_to_prestige(&mut piecemeal);
        assert_eq!(piecemeal.prestige(), Ok(()));
        let mut lump = state();
        ready_to_prestige(&mut lump);
        assert_eq!(lump.prestige(), Ok(()));

        for _ in 0..SPAN {
            piecemeal.tick(IDLE);
        }
        lump.credit_auto_mining(SPAN);

        assert_eq!(
            lump.player().get_inventory().raw_value(Material::Stone),
            piecemeal
                .player()
                .get_inventory()
                .raw_value(Material::Stone)
        );
        assert_eq!(lump.auto_common_progress, piecemeal.auto_common_progress);
        assert_eq!(lump.auto_value_progress, piecemeal.auto_value_progress);
    }

    /// Ranks stack, and the price climbs with them — the loop `docs/ROADMAP.md` leaves
    /// deliberately endless.
    #[test]
    fn a_second_prestige_costs_more_than_the_first() {
        let mut state = state();
        for expected in 1..=2 {
            ready_to_prestige(&mut state);
            assert_eq!(state.prestige(), Ok(()));
            assert_eq!(state.player().get_prestige(), expected);
        }
        assert!(prestige::cost(1).lines()[0].compressed > prestige::cost(0).lines()[0].compressed);
    }
}
