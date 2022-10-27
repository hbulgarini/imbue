#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use common_types::CurrencyId;
/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>
use frame_support::{
    pallet_prelude::*,
    storage::bounded_btree_map::BoundedBTreeMap,
    traits::{ConstU32, EnsureOrigin},
    transactional, PalletId,
};
use frame_system::pallet_prelude::*;
use orml_traits::MultiCurrency;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_runtime::traits::AccountIdConversion;
use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

#[cfg(test)]
mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::*;

pub mod impls;
pub use impls::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config:
        frame_system::Config + pallet_identity::Config + pallet_timestamp::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        type PalletId: Get<PalletId>;

        type AuthorityOrigin: EnsureOrigin<Self::Origin>;

        type MultiCurrency: MultiCurrency<AccountIdOf<Self>, CurrencyId = CurrencyId>;

        type MaxProjectsPerRound: Get<u32>;

        type MaxWithdrawalExpiration: Get<Self::BlockNumber>;

        type WeightInfo: WeightInfo;

        /// The amount of time given ,up to point of decision, when a vote of no confidence is held.
        type NoConfidenceTimeLimit: Get<Self::BlockNumber>;

        /// The minimum percentage of votes, inclusive, that is required for a vote to pass.  
        type PercentRequiredForVoteToPass: Get<u8>;
    }

    #[pallet::type_value]
    pub fn InitialMilestoneVotingWindow() -> u32 {
        100800u32
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::storage]
    #[pallet::getter(fn projects)]
    pub type Projects<T: Config> = StorageMap<
        _,
        Identity,
        ProjectKey,
        Project<T::AccountId, BalanceOf<T>, T::BlockNumber, TimestampOf<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn whitelist_spots)]
    pub type WhitelistSpots<T: Config> =
        StorageMap<_, Identity, ProjectKey, BTreeMap<T::AccountId, BalanceOf<T>>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn user_votes)]
    pub(super) type UserVotes<T: Config> = StorageMap<
        _,
        Identity,
        (T::AccountId, ProjectKey, MilestoneKey, RoundKey),
        bool,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn milestone_votes)]
    pub(super) type MilestoneVotes<T: Config> =
        StorageMap<_, Identity, (ProjectKey, MilestoneKey), Vote<BalanceOf<T>>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn no_confidence_votes)]
    pub(super) type NoConfidenceVotes<T: Config> =
        StorageMap<_, Identity, ProjectKey, Vote<BalanceOf<T>>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn project_count)]
    pub type ProjectCount<T> = StorageValue<_, ProjectKey, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn rounds)]
    pub type Rounds<T> = StorageMap<_, Identity, RoundKey, Option<RoundOf<T>>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn round_count)]
    pub type RoundCount<T> = StorageValue<_, RoundKey, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn max_project_count_per_round)]
    pub type MaxProjectCountPerRound<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn milestone_voting_window)]
    pub type MilestoneVotingWindow<T> =
        StorageValue<_, u32, ValueQuery, InitialMilestoneVotingWindow>;

    #[pallet::storage]
    #[pallet::getter(fn withdrawal_expiration)]
    pub type WithdrawalExpiration<T> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn is_identity_required)]
    pub type IsIdentityRequired<T> = StorageValue<_, bool, ValueQuery>;

    // Pallets use events to inform users when important changes are made.
    // https://substrate.dev/docs/en/knowledgebase/runtime/events
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProjectCreated(
            T::AccountId,
            Vec<u8>,
            ProjectKey,
            BalanceOf<T>,
            common_types::CurrencyId,
        ),
        FundingRoundCreated(RoundKey, Vec<ProjectKey>),
        VotingRoundCreated(RoundKey, Vec<ProjectKey>),
        MilestoneSubmitted(T::AccountId, ProjectKey, MilestoneKey),
        ContributeSucceeded(
            T::AccountId,
            ProjectKey,
            BalanceOf<T>,
            common_types::CurrencyId,
            T::BlockNumber,
        ),
        ProjectCancelled(RoundKey, ProjectKey),
        ProjectFundsWithdrawn(T::AccountId, ProjectKey, BalanceOf<T>, CurrencyId),
        ProjectApproved(RoundKey, ProjectKey),
        RoundCancelled(RoundKey),
        VoteComplete(T::AccountId, ProjectKey, MilestoneKey, bool, T::BlockNumber),
        MilestoneApproved(T::AccountId, ProjectKey, MilestoneKey, T::BlockNumber),
        WhitelistAdded(ProjectKey, T::BlockNumber),
        WhitelistRemoved(ProjectKey, T::BlockNumber),
        ProjectLockedFundsRefunded(ProjectKey, BalanceOf<T>),
        /// You have created a vote of no confidence.
        NoConfidenceRoundCreated(RoundKey, ProjectKey),
        /// You have voted upon a round of no confidence.
        NoConfidenceRoundVotedUpon(RoundKey, ProjectKey),
        /// You have finalised a vote of no confidence.
        NoConfidenceRoundFinalised(RoundKey, ProjectKey),
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        /// Contribution has exceeded the maximum capacity of the project.
        ContributionMustBeLowerThanMaxCap,
        /// This block number must be later than the current.
        EndBlockNumberInvalid,
        /// The starting block number must be before the ending block number.
        EndTooEarly,
        /// Required identity not found.
        IdentityNeeded,
        /// Input parameter is invalid
        InvalidParam,
        /// There are no avaliable funds to withdraw.
        NoAvailableFundsToWithdraw,
        /// Your account does not have the correct authority.
        InvalidAccount,
        /// Project does not exist.
        ProjectDoesNotExist,
        /// Project name is a mandatory field.
        ProjectNameIsMandatory,
        /// Project logo is a mandatory field.
        LogoIsMandatory,
        /// Project description is a mandatory field.
        ProjectDescriptionIsMandatory,
        /// Website url is a mandatory field.
        WebsiteURLIsMandatory,
        /// Milestones totals do not add up to 100%.
        MilestonesTotalPercentageMustEqual100,
        MilestoneDoesNotExist,
        /// Currently no active round to participate in.
        NoActiveRound,
        // TODO: NOT IN USE
        NoActiveProject,
        /// There was an overflow.
        Overflow,
        /// A project must be approved before the submission of milestones.
        OnlyApprovedProjectsCanSubmitMilestones,
        /// Only contributors can vote.
        OnlyContributorsCanVote,
        /// You do not have permission to do this.
        UserIsNotInitator,
        /// You do not have permission to do this.
        OnlyInitiatorOrAdminCanApproveMilestone,
        /// You do not have permission to do this.
        OnlyWhitelistedAccountsCanContribute,
        // TODO: not in use
        ProjectAmountExceed,
        /// The selected project does not exist in the round.
        ProjectNotInRound,
        // TODO: not in use.
        ProjectWithdrawn,
        // TODO: not in use.
        ProjectApproved,
        /// Parameter limit exceeded.
        ParamLimitExceed,
        /// Round has already started and cannot be modified.
        RoundStarted,
        /// Round stll in progress.
        RoundNotEnded,
        /// There was a processing error when finding the round.
        RoundNotProcessing,
        /// Round has been cancelled.
        RoundCanceled,
        // TODO: not in use.
        StartBlockNumberTooSmall,
        /// You have already voted on this round.
        VoteAlreadyExists,
        /// The voting threshhold has not been met.
        MilestoneVotingNotComplete,
        // TODO: not in use
        WithdrawalExpirationExceed,
        /// The given key must exist in storage.
        KeyNotFound,
        /// The input vector must exceed length zero.
        LengthMustExceedZero,
        /// The voting threshold has not been met.
        VoteThresholdNotMet,
        /// The project must be approved.
        ProjectApprovalRequired,
    }
    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Step 1 (INITATOR)
        /// Create project.
        #[pallet::weight(<T as Config>::WeightInfo::create_project())]
        pub fn create_project(
            origin: OriginFor<T>,
            name: BoundedStringField,
            logo: BoundedStringField,
            description: BoundedDescriptionField,
            website: BoundedDescriptionField,
            proposed_milestones: BoundedProposedMilestones,
            required_funds: BalanceOf<T>,
            currency_id: common_types::CurrencyId,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            // Validation
            ensure!(!name.is_empty(), Error::<T>::ProjectNameIsMandatory);
            ensure!(!logo.is_empty(), Error::<T>::LogoIsMandatory);
            ensure!(
                !description.is_empty(),
                Error::<T>::ProjectDescriptionIsMandatory
            );
            ensure!(!website.is_empty(), Error::<T>::WebsiteURLIsMandatory);

            let mut total_percentage = 0;
            for milestone in proposed_milestones.iter() {
                total_percentage += milestone.percentage_to_unlock;
            }
            ensure!(
                total_percentage == 100,
                Error::<T>::MilestonesTotalPercentageMustEqual100
            );

            Self::new_project(
                who,
                name,
                logo,
                description,
                website,
                proposed_milestones,
                required_funds,
                currency_id,
            )
        }

        /// Step 1.5 (INITATOR)
        /// Add whitelist to a project
        #[pallet::weight(<T as Config>::WeightInfo::create_project())]
        pub fn add_project_whitelist(
            origin: OriginFor<T>,
            project_key: ProjectKey,
            new_whitelist_spots: BoundedWhitelistSpots<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::ensure_initator(who, project_key)?;
            let mut project_whitelist_spots =
                WhitelistSpots::<T>::get(project_key).unwrap_or(BTreeMap::new());
            project_whitelist_spots.extend(new_whitelist_spots);
            <WhitelistSpots<T>>::insert(project_key, project_whitelist_spots);
            let now = <frame_system::Pallet<T>>::block_number();
            Self::deposit_event(Event::WhitelistAdded(project_key, now));
            Ok(().into())
        }

        /// Step 1.5 (INITATOR)
        /// Remove a whitelist
        #[pallet::weight(<T as Config>::WeightInfo::create_project())]
        pub fn remove_project_whitelist(
            origin: OriginFor<T>,
            project_key: ProjectKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::ensure_initator(who, project_key)?;
            <WhitelistSpots<T>>::remove(project_key);
            let now = <frame_system::Pallet<T>>::block_number();
            Self::deposit_event(Event::WhitelistRemoved(project_key, now));
            Ok(().into())
        }

        /// Step 2 (ADMIN)
        /// Schedule a round
        /// project_keys: the projects were selected for this round
        #[pallet::weight(<T as Config>::WeightInfo::schedule_round(MaxProjectCountPerRound::<T>::get()))]
        pub fn schedule_round(
            origin: OriginFor<T>,
            start: T::BlockNumber,
            end: T::BlockNumber,
            project_keys: BoundedProjectKeys,
            round_type: RoundType,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            let now = <frame_system::Pallet<T>>::block_number();
            // The number of items cannot exceed the maximum
            // ensure!(project_keyes.len() as u32 <= MaxProjectCountPerRound::<T>::get(), Error::<T>::ProjectAmountExceed);
            // The end block must be greater than the start block
            ensure!(end > start, Error::<T>::EndTooEarly);
            // Both the starting block number and the ending block number must be greater than the current number of blocks
            ensure!(end > now, Error::<T>::EndBlockNumberInvalid);
            ensure!(!project_keys.is_empty(), Error::<T>::LengthMustExceedZero);

            // Project keys is bounded to 5 projects maximum.
            let max_project_key = project_keys.iter().max().unwrap();
            Projects::<T>::get(&max_project_key).ok_or(Error::<T>::ProjectDoesNotExist)?;
            Self::new_round(start, end, project_keys, round_type)
        }

        /// Step 2.5 (ADMIN)
        /// Cancel a round
        /// This round must have not started yet
        #[pallet::weight(<T as Config>::WeightInfo::cancel_round())]
        pub fn cancel_round(
            origin: OriginFor<T>,
            round_key: RoundKey,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            let now = <frame_system::Pallet<T>>::block_number();
            let mut round = <Rounds<T>>::get(round_key).ok_or(Error::<T>::NoActiveRound)?;

            // Ensure current round is not started
            ensure!(round.start > now, Error::<T>::RoundStarted);
            // This round cannot be cancelled
            ensure!(!round.is_canceled, Error::<T>::RoundCanceled);

            round.is_canceled = true;
            <Rounds<T>>::insert(round_key, Some(round));

            Self::deposit_event(Event::RoundCancelled(round_key));

            Ok(().into())
        }

        /// Step 3 (CONTRIBUTOR/FUNDER)
        /// Contribute to a project
        #[pallet::weight(<T as Config>::WeightInfo::contribute())]
        #[transactional]
        pub fn contribute(
            origin: OriginFor<T>,
            round_key: Option<RoundKey>,
            project_key: ProjectKey,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let contribution_round_key = round_key.unwrap_or(RoundCount::<T>::get());
            Self::new_contribution(who, contribution_round_key, project_key, value)
        }

        /// Step 4 (ADMIN)
        /// Approve project
        /// If the project is approved, the project initator can withdraw funds for approved milestones
        #[pallet::weight(<T as Config>::WeightInfo::approve())]
        pub fn approve(
            origin: OriginFor<T>,
            round_key: Option<RoundKey>,
            project_key: ProjectKey,
            milestone_keys: Option<BoundedMilestoneKeys>,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            let approval_round_key = round_key.unwrap_or(RoundCount::<T>::get());
            Self::do_approve(project_key, approval_round_key, milestone_keys)
        }

        /// Step 5 (INITATOR)
        #[pallet::weight(<T as Config>::WeightInfo::submit_milestone())]
        pub fn submit_milestone(
            origin: OriginFor<T>,
            project_key: ProjectKey,
            milestone_key: MilestoneKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::new_milestone_submission(who, project_key, milestone_key)
        }

        /// Step 6 (CONTRIBUTOR/FUNDER)
        /// Vote on a milestone
        #[pallet::weight(<T as Config>::WeightInfo::contribute())]
        pub fn vote_on_milestone(
            origin: OriginFor<T>,
            project_key: ProjectKey,
            milestone_key: MilestoneKey,
            round_key: Option<RoundKey>,
            approve_milestone: bool,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let voting_round_key = round_key.unwrap_or(RoundCount::<T>::get());
            Self::new_milestone_vote(
                who,
                project_key,
                milestone_key,
                voting_round_key,
                approve_milestone,
            )
        }

        /// Step 7 (INITATOR)
        #[pallet::weight(<T as Config>::WeightInfo::submit_milestone())]
        pub fn finalise_milestone_voting(
            origin: OriginFor<T>,
            project_key: ProjectKey,
            milestone_key: MilestoneKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_finalise_milestone_voting(who, project_key, milestone_key)
        }

        /// Step 8 (INITATOR)
        /// Withdraw
        #[pallet::weight(<T as Config>::WeightInfo::withdraw())]
        pub fn withdraw(
            origin: OriginFor<T>,
            project_key: ProjectKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::new_withdrawal(who, project_key)
        }

        // TODO: BENCHMARK
        #[pallet::weight(<T as Config>::WeightInfo::withdraw())]
        pub fn raise_vote_of_no_confidence(
            origin: OriginFor<T>,
            project_key: ProjectKey,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::raise_no_confidence_round(who, project_key)
        }

        // TODO: BENCHMARK
        #[pallet::weight(<T as Config>::WeightInfo::withdraw())]
        pub fn vote_on_no_confidence_round(
            origin: OriginFor<T>,
            round_key: Option<RoundKey>,
            project_key: ProjectKey,
            is_yay: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let voting_round_key = round_key.unwrap_or(RoundCount::<T>::get());
            Self::add_vote_no_confidence(who, voting_round_key, project_key, is_yay)
        }

        // TODO: BENCHMARK
        #[pallet::weight(<T as Config>::WeightInfo::withdraw())]
        pub fn finalise_no_confidence_round(
            origin: OriginFor<T>,
            round_key: Option<RoundKey>,
            project_key: ProjectKey,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let voting_round_key = round_key.unwrap_or(RoundCount::<T>::get());
            Self::call_finalise_no_confidence_vote(
                who,
                voting_round_key,
                project_key,
                T::PercentRequiredForVoteToPass::get(),
            )
        }

        // Root Extrinsics:

        /// Set max project count per round
        #[pallet::weight(<T as Config>::WeightInfo::set_max_project_count_per_round(T::MaxProjectsPerRound::get()))]
        pub fn set_max_project_count_per_round(
            origin: OriginFor<T>,
            max_project_count_per_round: u32,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            ensure!(
                max_project_count_per_round > 0
                    || max_project_count_per_round <= T::MaxProjectsPerRound::get(),
                Error::<T>::ParamLimitExceed
            );
            MaxProjectCountPerRound::<T>::put(max_project_count_per_round);

            Ok(().into())
        }

        /// Set milestone voting window
        #[pallet::weight(<T as Config>::WeightInfo::set_max_project_count_per_round(T::MaxProjectsPerRound::get()))]
        pub fn set_milestone_voting_window(
            origin: OriginFor<T>,
            new_milestone_voting_window: u32,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            ensure!(
                new_milestone_voting_window > 0,
                Error::<T>::ParamLimitExceed
            );
            MilestoneVotingWindow::<T>::put(new_milestone_voting_window);

            Ok(().into())
        }

        /// Set withdrawal expiration
        #[pallet::weight(<T as Config>::WeightInfo::set_withdrawal_expiration())]
        pub fn set_withdrawal_expiration(
            origin: OriginFor<T>,
            withdrawal_expiration: T::BlockNumber,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            ensure!(
                withdrawal_expiration > (0_u32).into(),
                Error::<T>::InvalidParam
            );
            <WithdrawalExpiration<T>>::put(withdrawal_expiration);

            Ok(().into())
        }

        /// set is_identity_required
        #[pallet::weight(<T as Config>::WeightInfo::set_is_identity_required())]
        pub fn set_is_identity_required(
            origin: OriginFor<T>,
            is_identity_required: bool,
        ) -> DispatchResultWithPostInfo {
            T::AuthorityOrigin::ensure_origin(origin)?;
            IsIdentityRequired::<T>::put(is_identity_required);

            Ok(().into())
        }

        /// Ad Hoc Step (ADMIN)
        /// Refund
        #[pallet::weight(<T as Config>::WeightInfo::refund())]
        pub fn refund(origin: OriginFor<T>, project_key: ProjectKey) -> DispatchResultWithPostInfo {
            //ensure only admin can perform refund
            T::AuthorityOrigin::ensure_origin(origin)?;
            Self::do_refund(project_key)
        }
    }
}

// The Constants associated with the bounded parameters
type MaxStringFieldLen = ConstU32<255>;
type MaxProjectKeys = ConstU32<1000>;
type MaxMilestoneKeys = ConstU32<100>;
type MaxProposedMilestones = ConstU32<100>;
type MaxDescriptionField = ConstU32<5000>;
type MaxWhitelistPerProject = ConstU32<10000>;

pub type RoundKey = u32;
pub type ProjectKey = u32;
pub type MilestoneKey = u32;
type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type BalanceOf<T> = <<T as Config>::MultiCurrency as MultiCurrency<AccountIdOf<T>>>::Balance;
type RoundOf<T> = Round<<T as frame_system::Config>::BlockNumber>;
type TimestampOf<T> = <T as pallet_timestamp::Config>::Moment;

// These are the bounded types which are suitable for handling user input due to their restriction of vector length.
type BoundedWhitelistSpots<T> =
    BoundedBTreeMap<AccountIdOf<T>, BalanceOf<T>, MaxWhitelistPerProject>;
type BoundedProjectKeys = BoundedVec<ProjectKey, MaxProjectKeys>;
type BoundedMilestoneKeys = BoundedVec<ProjectKey, MaxMilestoneKeys>;
type BoundedStringField = BoundedVec<u8, MaxStringFieldLen>;
type BoundedProposedMilestones = BoundedVec<ProposedMilestone, MaxProposedMilestones>;
type BoundedDescriptionField = BoundedVec<u8, MaxDescriptionField>;

#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub enum RoundType {
    ContributionRound,
    VotingRound,
    VoteOfNoConfidence,
}

/// The round struct contains all the data associated with a given round.
/// A round may include multiple projects.
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Round<BlockNumber> {
    start: BlockNumber,
    end: BlockNumber,
    project_keys: Vec<ProjectKey>,
    round_type: RoundType,
    is_canceled: bool,
}

impl<BlockNumber: From<u32>> Round<BlockNumber> {
    fn new(
        start: BlockNumber,
        end: BlockNumber,
        project_keys: Vec<ProjectKey>,
        round_type: RoundType,
    ) -> Round<BlockNumber> {
        Round {
            start,
            end,
            project_keys,
            is_canceled: false,
            round_type,
        }
    }
}

/// The contribution users made to a project project.
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct ProposedMilestone {
    name: BoundedStringField,
    percentage_to_unlock: u32,
}

/// The contribution users made to a project project.
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Milestone {
    project_key: ProjectKey,
    milestone_key: MilestoneKey,
    name: Vec<u8>,
    percentage_to_unlock: u32,
    is_approved: bool,
}

/// The vote struct is used to
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Vote<Balance> {
    yay: Balance,
    nay: Balance,
    is_approved: bool,
}

/// The struct that holds the descriptive properties of a project.
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Project<AccountId, Balance, BlockNumber, Timestamp> {
    name: Vec<u8>,
    logo: Vec<u8>,
    description: Vec<u8>,
    website: Vec<u8>,
    milestones: BTreeMap<MilestoneKey, Milestone>,
    /// A collection of the accounts which have contributed and their contributions.
    contributions: BTreeMap<AccountId, Contribution<Balance, Timestamp>>,
    currency_id: common_types::CurrencyId,
    required_funds: Balance,
    withdrawn_funds: Balance,
    raised_funds: Balance,
    /// The account that will receive the funds if the campaign is successful
    initiator: AccountId,
    create_block_number: BlockNumber,
    approved_for_funding: bool,
    funding_threshold_met: bool,
    cancelled: bool,
}

/// The contribution users made to a proposal project.
#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Contribution<Balance, Timestamp> {
    /// Contribution value
    value: Balance,
    /// Timestamp of the last contribution
    timestamp: Timestamp,
}

#[derive(Encode, Decode, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Whitelist<AccountId, Balance> {
    who: AccountId,
    max_cap: Balance,
}
