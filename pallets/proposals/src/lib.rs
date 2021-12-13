#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>

#[cfg(feature = "std")]
use frame_support::traits::GenesisBuild;
use frame_support::{
	pallet_prelude::*, PalletId,
	log,
	traits::{Currency, ReservableCurrency, ExistenceRequirement, WithdrawReasons},
};
use codec::{Encode, Decode};
use sp_std::prelude::*;
use integer_sqrt::IntegerSquareRoot;
use sp_runtime::{traits::{AccountIdConversion,Saturating},Perbill};
pub use pallet::*;
use scale_info::TypeInfo;


#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::*;

const MAX_STRING_FIELD_LENGTH: usize = 256;

#[frame_support::pallet]
pub mod pallet {
	use frame_system::pallet_prelude::*;
	use frame_support::pallet_prelude::*;
	use super::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_identity::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type PalletId: Get<PalletId>;

		type Currency: ReservableCurrency<Self::AccountId>;

		type MaxProposalsPerRound: Get<u32>;

		type MaxWithdrawalExpiration: Get<Self::BlockNumber>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	#[pallet::getter(fn projects)]
	pub type Projects<T> = StorageMap<_, Blake2_128Concat, ProjectIndex, Option<ProjectOf<T>>, ValueQuery>;

	#[pallet::storage]
    #[pallet::getter(fn user_votes)]
    pub(super) type UserVotes<T: Config> = StorageMap<_, Identity, (T::AccountId, ProjectIndex, MilestoneIndex), bool, ValueQuery>;


	#[pallet::storage]
    #[pallet::getter(fn milestone_votes)]
    pub(super) type MilestoneVotes<T: Config> = StorageMap<_, Identity, (ProjectIndex, MilestoneIndex), Vote<BalanceOf<T>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn project_count)]
	pub type ProjectCount<T> = StorageValue<_, ProjectIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn rounds)]
	pub type Rounds<T> = StorageMap<_, Blake2_128Concat, RoundIndex, Option<RoundOf<T>>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn round_count)]
	pub type RoundCount<T> = StorageValue<_, RoundIndex, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn max_proposal_count_per_round)]
	pub type MaxProposalCountPerRound<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn withdrawal_expiration)]
	pub type WithdrawalExpiration<T> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn is_identity_required)]
	pub type IsIdentityRequired<T> = StorageValue<_, bool, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub init_max_proposal_count_per_round: u32,
		pub init_withdrawal_expiration: BlockNumberFor<T>,
		pub init_is_identity_required: bool,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				init_max_proposal_count_per_round: 5,
				init_withdrawal_expiration: Default::default(),
				init_is_identity_required: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			MaxProposalCountPerRound::<T>::put(self.init_max_proposal_count_per_round);
			WithdrawalExpiration::<T>::put(self.init_withdrawal_expiration);
			IsIdentityRequired::<T>::put(self.init_is_identity_required);
		}
	}

	// Pallets use events to inform users when important changes are made.
	// https://substrate.dev/docs/en/knowledgebase/runtime/events
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		ProjectCreated(ProjectIndex),
		RoundCreated(RoundIndex),
		ContributeSucceed(T::AccountId, ProjectIndex, BalanceOf<T>, T::BlockNumber),
		ProposalCanceled(RoundIndex, ProjectIndex),
		ProposalWithdrawn(RoundIndex, ProjectIndex, BalanceOf<T>),
		ProposalApproved(RoundIndex, ProjectIndex),
		RoundCanceled(RoundIndex),
		FundSucceed(),
		RoundFinalized(RoundIndex),
		VoteComplete(T::AccountId, ProjectIndex, MilestoneIndex, bool, T::BlockNumber),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		EndBlockNumberInvalid,
		EndTooEarly,
		IdentityNeeded,
		InvalidParam,
		InvalidAccount,
		InvalidProjectIndexes,
		MilestonesTotalPercentageMustEqual100,
		NotEnoughFund,
		/// Error names should be descriptive.
		NoneValue,
		NoActiveRound,
		NoActiveProposal,
		/// There was an overflow.
		///
		Overflow,
		OnlyContributorsCanVote,
		ProposalAmountExceed,
		ProposalCanceled,
		ProposalWithdrawn,
		ProposalApproved,
		ProposalNotApproved,
		ParamLimitExceed,
		RoundStarted,
		RoundNotEnded,
		RoundNotProcessing,
		RoundCanceled,
		RoundFinalized,
		RoundNotFinalized,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
		StartBlockNumberInvalid,
		StartBlockNumberTooSmall,
		VoteAlreadyExists,
		WithdrawalExpirationExceed,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create project
		#[pallet::weight(<T as Config>::WeightInfo::create_project())]
		pub fn create_project(origin: OriginFor<T>, name: Vec<u8>, logo: Vec<u8>, description: Vec<u8>, website: Vec<u8>, proposed_milestones: Vec<ProposedMilestone>, required_funds: BalanceOf<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// Check if identity is required
			let is_identity_needed = IsIdentityRequired::<T>::get();
			if is_identity_needed {
				let identity = pallet_identity::Pallet::<T>::identity(who.clone()).ok_or(Error::<T>::IdentityNeeded)?;
				let mut is_found_judgement = false;
				for judgement in identity.judgements.iter() {
					if judgement.1 == pallet_identity::Judgement::Reasonable || judgement.1 == pallet_identity::Judgement::KnownGood {
						is_found_judgement = true;
						break;
					}
				}
				ensure!(is_found_judgement, Error::<T>::IdentityNeeded);
			}

			// Validation
			ensure!(name.len() > 0, Error::<T>::InvalidParam);
			ensure!(logo.len() > 0, Error::<T>::InvalidParam);
			ensure!(description.len() > 0, Error::<T>::InvalidParam);
			ensure!(website.len() > 0, Error::<T>::InvalidParam);

			// let mut total_percentage = 0;
			// for milestone in milestones.iter() {
			// 	log::info!("*********************** percentage for this milestone is {:?} ***********************",milestone.percentage_to_unlock);
			// 	total_percentage += milestone.percentage_to_unlock;
			// }
			// ensure!(total_percentage == 100, Error::<T>::MilestonesTotalPercentageMustEqual100);

			// ensure!(name.len() <= MAX_STRING_FIELD_LENGTH, Error::<T>::ParamLimitExceed);
			// ensure!(logo.len() <= MAX_STRING_FIELD_LENGTH, Error::<T>::ParamLimitExceed);
			// ensure!(description.len() <= MAX_STRING_FIELD_LENGTH, Error::<T>::ParamLimitExceed);
			// ensure!(website.len() <= MAX_STRING_FIELD_LENGTH, Error::<T>::ParamLimitExceed);
			
			let project_index = ProjectCount::<T>::get();
			let next_project_index = project_index.checked_add(1).ok_or(Error::<T>::Overflow)?;

			let mut milestones = Vec::new();
			let mut milestone_index:u32 = 0;

			// Fill in the proposals structure in advance
			for milestone in proposed_milestones {
				// let project =  <Projects<T>>::get(project_index).unwrap();
				milestones.push(Milestone {
					project_index: project_index,
					milestone_index:milestone_index,
					name: milestone.name,
					percentage_to_unlock: milestone.percentage_to_unlock,
					is_approved: false,
				});
				milestone_index = milestone_index.checked_add(1).ok_or(Error::<T>::Overflow)?;
			}
 
			// Create a proposal 
			let project = ProjectOf::<T> {
				name: name,
				logo: logo,
				description: description,
				website: website,
				milestones: milestones,
				contributions: Vec::new(),
				required_funds: required_funds,
				withdrawn_funds:(0 as u32).into(), 
				owner: who,
				create_block_number: <frame_system::Pallet<T>>::block_number(),
			};

			// Add proposal to list
			<Projects<T>>::insert(project_index, Some(project));
			ProjectCount::<T>::put(next_project_index);

			Self::deposit_event(Event::ProjectCreated(project_index));

			Ok(().into())
		}

		/// Schedule a round
		/// proposal_indexes: the proposals were selected for this round
		#[pallet::weight(<T as Config>::WeightInfo::schedule_round(MaxProposalCountPerRound::<T>::get()))]
		pub fn schedule_round(origin: OriginFor<T>, start: T::BlockNumber, end: T::BlockNumber, project_index: ProjectIndex, milestone_indexes: Vec<MilestoneIndex>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let now = <frame_system::Pallet<T>>::block_number();

			// The number of items cannot exceed the maximum
			// ensure!(project_indexes.len() as u32 <= MaxProposalCountPerRound::<T>::get(), Error::<T>::ProposalAmountExceed);
			// The end block must be greater than the start block
			ensure!(end > start, Error::<T>::EndTooEarly);
			// Both the starting block number and the ending block number must be greater than the current number of blocks
			// ensure!(start > now, Error::<T>::StartBlockNumberInvalid);
			ensure!(end > now, Error::<T>::EndBlockNumberInvalid);

			// project_index should be smaller than project count
			let project_count = ProjectCount::<T>::get();
			ensure!(project_index < project_count, Error::<T>::InvalidProjectIndexes);

			// Find the last valid round
			let mut last_valid_round: Option<RoundOf::<T>> = None;
			let index = RoundCount::<T>::get();

			for _i in (0..index).rev() {
				let round = <Rounds<T>>::get(index-1).unwrap();
				if !round.is_canceled {
					last_valid_round = Some(round);
					break;
				}
			}

			// The start time must be greater than the end time of the last valid round
			match last_valid_round {
				Some(round) => {
					ensure!(start > round.end, Error::<T>::StartBlockNumberTooSmall);
				},
				None => {}
			}

			let next_index = index.checked_add(1).ok_or(Error::<T>::Overflow)?;

			let round = RoundOf::<T>::new(start, end, project_index, milestone_indexes.clone());

			for milestone_index in milestone_indexes {
				// Initialise voting
				let vote = Vote {
					yay: (0 as u32).into(),
					nay: (0 as u32).into(),
					is_approved: false
				};
				let vote_lookup_key = (project_index, milestone_index);
				<MilestoneVotes<T>>::insert(vote_lookup_key,vote);
			}

			// Add proposal round to list
			<Rounds<T>>::insert(index, Some(round));
			RoundCount::<T>::put(next_index);

			Self::deposit_event(Event::RoundCreated(index));

			Ok(().into())
		}

		/// Cancel a round
		/// This round must have not started yet
		#[pallet::weight(<T as Config>::WeightInfo::cancel_round())]
		pub fn cancel_round(origin: OriginFor<T>, round_index: RoundIndex) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let now = <frame_system::Pallet<T>>::block_number();
			let count = RoundCount::<T>::get();
			let mut round = <Rounds<T>>::get(round_index).ok_or(Error::<T>::NoActiveRound)?;

			// Ensure current round is not started
			ensure!(round.start > now, Error::<T>::RoundStarted);
			// This round cannot be cancelled
			ensure!(!round.is_canceled, Error::<T>::RoundCanceled);

			round.is_canceled = true;
			<Rounds<T>>::insert(round_index, Some(round.clone()));

			Self::deposit_event(Event::RoundCanceled(count-1));

			Ok(().into())
		}

		/// Vote on a milestone
		#[pallet::weight(<T as Config>::WeightInfo::contribute())]
		pub fn vote_on_milestone(origin: OriginFor<T>, project_index: ProjectIndex, milestone_index: MilestoneIndex, approve_milestone: bool) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let project_count = ProjectCount::<T>::get();
			ensure!(project_index < project_count, Error::<T>::InvalidParam);
			let now = <frame_system::Pallet<T>>::block_number();
			
			// round list must be not none
			let round_index = RoundCount::<T>::get();
			ensure!(round_index > 0, Error::<T>::NoActiveRound);

			// Find processing round
			let mut processing_round: Option<RoundOf::<T>> = None;
			for i in (0..round_index).rev() {
				let round = <Rounds<T>>::get(i).unwrap();
				if !round.is_canceled && round.start < now && round.end > now {
					processing_round = Some(round);
				}
			}
			let mut round = processing_round.ok_or(Error::<T>::RoundNotProcessing)?;

			// Find proposal by index
			let mut found_proposal: Option<&mut ProposalOf::<T>> = None;
			for proposal in round.proposals.iter_mut() {
				if proposal.project_index == project_index {
					found_proposal = Some(proposal);
					break;
				}
			}

			let proposal = found_proposal.ok_or(Error::<T>::NoActiveProposal)?;
			let project = Projects::<T>::get(project_index).ok_or(Error::<T>::NoActiveProposal)?;

			ensure!(!proposal.is_canceled, Error::<T>::ProposalCanceled);
			let mut existing_contributer = false;
			let mut contribution_amount: BalanceOf<T>  = (0 as u32).into();

			// Find previous contribution by account_id
			// If you have contributed before, then add to that contribution. Otherwise join the list.
			for contribution in project.contributions.clone().iter_mut() {
				if contribution.account_id == who {
					existing_contributer = true;
					contribution_amount = contribution.value;
					break;
				}
			}

			ensure!(existing_contributer, Error::<T>::OnlyContributorsCanVote);
			let vote_lookup_key = (who.clone(), project_index, milestone_index);

			let vote_exists = UserVotes::<T>::contains_key(vote_lookup_key.clone());
			ensure!(!vote_exists, Error::<T>::VoteAlreadyExists);
			
			<UserVotes<T>>::insert(vote_lookup_key,approve_milestone);

			let current_vote = <MilestoneVotes<T>>::get((project_index, milestone_index));

			if approve_milestone {
				let updated_vote = Vote {
					yay: current_vote.yay + contribution_amount,
					nay: current_vote.nay,
					is_approved: current_vote.is_approved
				};
				<MilestoneVotes<T>>::insert((project_index, milestone_index),updated_vote)

			} else {
				let updated_vote = Vote {
					yay: current_vote.yay,
					nay: current_vote.nay + contribution_amount,
					is_approved: current_vote.is_approved
				};
				<MilestoneVotes<T>>::insert((project_index, milestone_index),updated_vote)
			}


			<Rounds<T>>::insert(round_index-1, Some(round));
			Self::deposit_event(Event::VoteComplete(who, project_index, milestone_index, approve_milestone, now));

			Ok(().into())
		}

		/// Contribute a proposal
		#[pallet::weight(<T as Config>::WeightInfo::contribute())]
		pub fn contribute(origin: OriginFor<T>, project_index: ProjectIndex, value: BalanceOf<T>) -> DispatchResultWithPostInfo { 
			let who = ensure_signed(origin)?;
			ensure!(value > (0 as u32).into(), Error::<T>::InvalidParam);
			let project_count = ProjectCount::<T>::get();
			ensure!(project_index < project_count, Error::<T>::InvalidParam);
			let now = <frame_system::Pallet<T>>::block_number();
			
			// round list must be not none
			let round_index = RoundCount::<T>::get();
			ensure!(round_index > 0, Error::<T>::NoActiveRound);

			// Find processing round
			let mut processing_round: Option<RoundOf::<T>> = None;
			for i in (0..round_index).rev() {
				let round = <Rounds<T>>::get(i).unwrap();
				if !round.is_canceled && round.start < now && round.end > now {
					processing_round = Some(round);
				}
			}

			let mut round = processing_round.ok_or(Error::<T>::RoundNotProcessing)?;

			// Find proposal by index
			let mut found_proposal: Option<&mut ProposalOf::<T>> = None;
			for proposal in round.proposals.iter_mut() {
				if proposal.project_index == project_index {
					found_proposal = Some(proposal);
					break;
				}
			}

			let proposal = found_proposal.ok_or(Error::<T>::NoActiveProposal)?;
			ensure!(!proposal.is_canceled, Error::<T>::ProposalCanceled);

			// Find previous contribution by account_id
			// If you have contributed before, then add to that contribution. Otherwise join the list.
			let mut found_contribution: Option<&mut ContributionOf::<T>> = None;
			for contribution in proposal.contributions.iter_mut() {
				if contribution.account_id == who {
					found_contribution = Some(contribution);
					break;
				}
			}

			match found_contribution {
				Some(contribution) => {
					contribution.value += value;
				},
				None => {
					proposal.contributions.push(ContributionOf::<T> {
						account_id: who.clone(),
						value: value,
					});
				}
			}

			let project = Projects::<T>::get(project_index).ok_or(Error::<T>::NoActiveProposal)?;
			// Update project withdrawn funds
			let updated_project = ProjectOf::<T> {
				name: project.name,
				logo: project.logo,
				description: project.description,
				website: project.website,
				milestones: project.milestones,
				contributions:proposal.contributions.clone(),
				required_funds: project.required_funds,
				withdrawn_funds: project.withdrawn_funds,
				owner: project.owner,
				create_block_number: project.create_block_number,
			};
			// Add proposal to list
			<Projects<T>>::insert(project_index, Some(updated_project));

			// Transfer contribute to proposal account
			<T as Config>::Currency::transfer(
				&who,
				&Self::project_account_id(project_index),
				value,
				ExistenceRequirement::AllowDeath
			)?;
			
			<Rounds<T>>::insert(round_index-1, Some(round));

			Self::deposit_event(Event::ContributeSucceed(who, project_index, value, now));

			Ok(().into())
		}
		
		/// Approve project
		/// If the project is approve, the project owner can withdraw funds
		#[pallet::weight(<T as Config>::WeightInfo::approve())]
		pub fn approve(origin: OriginFor<T>, round_index: RoundIndex, project_index: ProjectIndex, milestone_indexes:  Vec<MilestoneIndex>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let mut round = <Rounds<T>>::get(round_index).ok_or(Error::<T>::NoActiveRound)?;
			ensure!(!round.is_canceled, Error::<T>::RoundCanceled);
			let proposals = &mut round.proposals;

			// The round must have ended
			let now = <frame_system::Pallet<T>>::block_number();
			// This round must be over
			ensure!(round.end < now, Error::<T>::RoundNotEnded);

			// Find proposal from list
			let mut found_proposal: Option<&mut ProposalOf::<T>> = None;
			for proposal in proposals.iter_mut() {
				if proposal.project_index == project_index {
					found_proposal = Some(proposal);
					break;
				}
			}
			let mut proposal = found_proposal.ok_or(Error::<T>::NoActiveProposal)?;

			// Can't let users vote in the cancered round
			ensure!(!proposal.is_canceled, Error::<T>::ProposalCanceled);
			// ensure!(!proposal.is_approved, Error::<T>::ProposalApproved);
			let project = Projects::<T>::get(project_index).ok_or(Error::<T>::NoActiveProposal)?;


			let mut milestones = Vec::new();

			// set is_approved
			proposal.is_approved = true;

			for mut milestone in project.milestones.into_iter() {
				for index in milestone_indexes.clone().into_iter() {
					if milestone.milestone_index == index {
						let vote = <MilestoneVotes<T>>::get((project_index, index));
						if vote.yay > vote.nay {
							milestone.is_approved = true;
						}
					}
				}
				milestones.push(milestone.clone());
			}

			// for milestone in proposal.milestones.
			proposal.withdrawal_expiration = now + <WithdrawalExpiration<T>>::get();

			<Rounds<T>>::insert(round_index, Some(round.clone()));

			// Update project milestones
			let updated_project = ProjectOf::<T> {
				name: project.name,
				logo: project.logo,
				description: project.description,
				website: project.website,
				milestones: milestones,
				contributions: project.contributions,
				required_funds: project.required_funds,
				withdrawn_funds: project.withdrawn_funds,
				owner: project.owner,
				create_block_number: project.create_block_number,
			};
			// Add proposal to list
			<Projects<T>>::insert(project_index, Some(updated_project));
			Self::deposit_event(Event::ProposalApproved(round_index, project_index));
			Ok(().into())
		}

		/// Withdraw
		#[pallet::weight(<T as Config>::WeightInfo::withdraw())]
		pub fn withdraw(origin: OriginFor<T>, round_index: RoundIndex, project_index: ProjectIndex) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let now = <frame_system::Pallet<T>>::block_number();

			// Only project owner can withdraw
			let project = Projects::<T>::get(project_index).ok_or(Error::<T>::NoActiveProposal)?;
			ensure!(who == project.owner, Error::<T>::InvalidAccount);

			let mut round = <Rounds<T>>::get(round_index).ok_or(Error::<T>::NoActiveRound)?;
			let mut found_proposal: Option<&mut ProposalOf::<T>> = None;
			for proposal in round.proposals.iter_mut() {
				if proposal.project_index == project_index {
					found_proposal = Some(proposal);
					break;
				}
			}

			let proposal = found_proposal.ok_or(Error::<T>::NoActiveProposal)?;
			// ensure!(now <= proposal.withdrawal_expiration, Error::<T>::WithdrawalExpirationExceed);

			// This proposal must not have distributed funds
			// ensure!(proposal.is_approved, Error::<T>::ProposalNotApproved);
			ensure!(!proposal.is_withdrawn, Error::<T>::ProposalWithdrawn);

			// Calculate contribution amount
			let mut total_contribution_amount: BalanceOf<T>  = (0 as u32).into();
			for contribution in project.contributions.iter() {
				let contribution_value = contribution.value;
				total_contribution_amount += contribution_value;
			}

			let mut unlocked_funds: BalanceOf<T>  = (0 as u32).into();
			for milestone in project.milestones.clone() {
				if milestone.is_approved {
					 unlocked_funds += (total_contribution_amount *  milestone.percentage_to_unlock.into())/100u32.into();
				}
			}

			let available_funds: BalanceOf<T> = unlocked_funds - project.withdrawn_funds;
			// ensure!(available_funds >  (0 as u32).into(), Error::<T>::InvalidParam);

			// Distribute contribution amount
			let _ = <T as Config>::Currency::resolve_into_existing(&project.owner, <T as Config>::Currency::withdraw(
				&Self::project_account_id(project_index),
				available_funds,
				WithdrawReasons::from(WithdrawReasons::TRANSFER),
				ExistenceRequirement::AllowDeath,
			)?);

			// Update project withdrawn funds
			let updated_project = ProjectOf::<T> {
				name: project.name,
				logo: project.logo,
				description: project.description,
				website: project.website,
				milestones: project.milestones,
				contributions:project.contributions,
				required_funds: project.required_funds,
				withdrawn_funds: available_funds,
				owner: project.owner,
				create_block_number: project.create_block_number,
			};
			// Add proposal to list
			<Projects<T>>::insert(project_index, Some(updated_project));

			// Set is_withdrawn
			proposal.is_withdrawn = true;
			proposal.withdrawal_expiration = now + <WithdrawalExpiration<T>>::get();

			<Rounds<T>>::insert(round_index, Some(round.clone()));

			Self::deposit_event(Event::ProposalWithdrawn(round_index, project_index, available_funds));

			Ok(().into())
		}

		/// Cancel a problematic project
		/// If the project is cancelled, users cannot donate to it, and project owner cannot withdraw funds.
		#[pallet::weight(<T as Config>::WeightInfo::cancel())]
		pub fn cancel(origin: OriginFor<T>, round_index: RoundIndex, project_index: ProjectIndex) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let mut round = <Rounds<T>>::get(round_index).ok_or(Error::<T>::NoActiveRound)?;

			// This round cannot be cancelled
			ensure!(!round.is_canceled, Error::<T>::RoundCanceled);

			let proposals = &mut round.proposals;

			let mut found_proposal: Option<&mut ProposalOf::<T>> = None;

			// Find proposal with project index
			for proposal in proposals.iter_mut() {
				if proposal.project_index == project_index {
					found_proposal = Some(proposal);
					break;
				}
			}

			let proposal = found_proposal.ok_or(Error::<T>::NoActiveProposal)?;

			// This proposal must not have canceled
			ensure!(!proposal.is_canceled, Error::<T>::ProposalCanceled);
			ensure!(!proposal.is_approved, Error::<T>::ProposalApproved);

			proposal.is_canceled = true;

			Rounds::<T>::insert(round_index, Some(round));

			Self::deposit_event(Event::ProposalCanceled(round_index, project_index));

			Ok(().into())
		}

		/// Set max proposal count per round
		#[pallet::weight(<T as Config>::WeightInfo::set_max_proposal_count_per_round(T::MaxProposalsPerRound::get()))]
		pub fn set_max_proposal_count_per_round(origin: OriginFor<T>, max_proposal_count_per_round: u32) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(max_proposal_count_per_round > 0 || max_proposal_count_per_round <= T::MaxProposalsPerRound::get(), Error::<T>::ParamLimitExceed);
			MaxProposalCountPerRound::<T>::put(max_proposal_count_per_round);

			Ok(().into())
		}

		/// Set withdrawal expiration
		#[pallet::weight(<T as Config>::WeightInfo::set_withdrawal_expiration())]
		pub fn set_withdrawal_expiration(origin: OriginFor<T>, withdrawal_expiration: T::BlockNumber) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			ensure!(withdrawal_expiration > (0 as u32).into(), Error::<T>::InvalidParam);
			<WithdrawalExpiration<T>>::put(withdrawal_expiration);

			Ok(().into())
		}

		/// set is_identity_required
		#[pallet::weight(<T as Config>::WeightInfo::set_is_identity_required())]
		pub fn set_is_identity_required(origin: OriginFor<T>, is_identity_required: bool) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			IsIdentityRequired::<T>::put(is_identity_required);

			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The account ID of the fund pot.
	///
	/// This actually does computation. If you need to keep using it, then make sure you cache the
	/// value and only call this once.
	pub fn account_id() -> T::AccountId {
		T::PalletId::get().into_account()
	}

	pub fn project_account_id(index: ProjectIndex) -> T::AccountId {
		T::PalletId::get().into_sub_account(index)
	}

	/// Get all projects
	pub fn get_projects() -> Vec<Project<AccountIdOf<T>, BalanceOf<T>, T::BlockNumber>> {
		let len = ProjectCount::<T>::get();
		let mut projects: Vec<Project<AccountIdOf<T>, BalanceOf<T>, T::BlockNumber>> = Vec::new();
		for i in 0..len {
			let project = <Projects<T>>::get(i).unwrap();
			projects.push(project);
		}
		projects
	}

	pub fn get_project(project_key: u32) -> Project<AccountIdOf<T>, BalanceOf<T>, T::BlockNumber> {
		let project = <Projects<T>>::get(project_key).unwrap();
		project
	}

}

pub type RoundIndex = u32;
pub type ProjectIndex = u32;
pub type MilestoneIndex = u32;


type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type BalanceOf<T> = <<T as Config>::Currency as Currency<AccountIdOf<T>>>::Balance;
type ProjectOf<T> = Project<AccountIdOf<T>, BalanceOf<T>, <T as frame_system::Config>::BlockNumber>;
type ContributionOf<T> = Contribution<AccountIdOf<T>, BalanceOf<T>>;
type RoundOf<T> = Round<AccountIdOf<T>, BalanceOf<T>, <T as frame_system::Config>::BlockNumber>;
type ProposalOf<T> = Proposal<AccountIdOf<T>, BalanceOf<T>, <T as frame_system::Config>::BlockNumber>;

/// Round struct
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Round<AccountId, Balance, BlockNumber> {
	start: BlockNumber,
	end: BlockNumber,
	proposals: Vec<Proposal<AccountId, Balance, BlockNumber>>,
	is_canceled: bool,
}

impl<AccountId, Balance: From<u32>, BlockNumber: From<u32>> Round<AccountId, Balance, BlockNumber> {
		fn new(start: BlockNumber, end: BlockNumber, project_index: ProjectIndex, milestone_indexes: Vec<MilestoneIndex>) -> Round<AccountId, Balance, BlockNumber> { 
			let mut proposal_round  = Round {
				start: start,
				end: end,
				proposals: Vec::new(),
				is_canceled: false,
			};

			// let project =  Self::get_project(project_index);

			// let mut round_milestones = Vec::new();



			// for milestone_index in milestone_indexes {
			// 	for milestone in project.milestones {
			// 		if milestone.milestone_index  == milestone_index {
			// 			round_milestones.push(milestone)
			// 		}
			// 	}
			// }
				


			proposal_round.proposals.push(Proposal {
				project_index: project_index,
				milestone_indexes: milestone_indexes,
				contributions: Vec::new(),
				is_approved: false,
				is_canceled: false,
				is_withdrawn: false,
				withdrawal_expiration: (0 as u32).into(),
			});
			proposal_round
	}
}
// Proposal in round
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Proposal<AccountId, Balance, BlockNumber> {
	project_index: ProjectIndex,
	milestone_indexes: Vec<MilestoneIndex>,
	contributions: Vec<Contribution<AccountId, Balance>>,
	is_approved: bool,
	is_canceled: bool,
	is_withdrawn: bool,
	withdrawal_expiration: BlockNumber,
}

/// The contribution users made to a proposal project.
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Contribution<AccountId, Balance> {
	account_id: AccountId,
	value: Balance,
}

/// The contribution users made to a proposal project.
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct ProposedMilestone {
	name: Vec<u8>,
	percentage_to_unlock: u32,
}

/// The contribution users made to a proposal project.
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Milestone {
	project_index: ProjectIndex,
	milestone_index: MilestoneIndex,
	name: Vec<u8>,
	percentage_to_unlock: u32,
	is_approved: bool
}

/// The contribution users made to a proposal project.
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Vote<Balance> {
	yay: Balance,
	nay: Balance,
	is_approved: bool
}

/// Project struct
#[derive(Encode, Decode, Default, PartialEq, Eq, Clone, Debug, TypeInfo)]
pub struct Project<AccountId, Balance, BlockNumber> {
	name: Vec<u8>,
	logo: Vec<u8>,
	description: Vec<u8>,
	website: Vec<u8>,
	milestones: Vec<Milestone>,
	contributions: Vec<Contribution<AccountId, Balance>>,
	required_funds: Balance,
	withdrawn_funds: Balance,
	/// The account that will receive the funds if the campaign is successful
	owner: AccountId,
	create_block_number: BlockNumber,
}

#[cfg(feature = "std")]
impl<T: Config> GenesisConfig<T> {
	/// Direct implementation of `GenesisBuild::build_storage`.
	///
	/// Kept in order not to break dependency.
	pub fn build_storage(&self) -> Result<sp_runtime::Storage, String> {
		<Self as GenesisBuild<T>>::build_storage(self)
	}

	/// Direct implementation of `GenesisBuild::assimilate_storage`.
	///
	/// Kept in order not to break dependency.
	pub fn assimilate_storage(
		&self,
		storage: &mut sp_runtime::Storage
	) -> Result<(), String> {
		<Self as GenesisBuild<T>>::assimilate_storage(self, storage)
	}
}
