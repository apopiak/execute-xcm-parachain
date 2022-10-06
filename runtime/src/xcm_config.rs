use super::{
	AccountId, AssetId, Assets, Balance, Balances, Call, Event, Origin, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime,
	WeightToFee, XcmpQueue,
};
use core::marker::PhantomData;
use frame_support::{
	log, match_types, parameter_types,
	traits::{Everything, Nothing, PalletInfoAccess},
};
use pallet_xcm::XcmPassthrough;
// use parachains_common::impls::AssetsFrom;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::{prelude::*, Weight as XCMWeight};
use xcm_builder::{
	AccountId32Aliases, AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, ConvertedConcreteAssetId, CurrencyAdapter,
	EnsureXcmOrigin, FixedWeightBounds, FungiblesAdapter, IsConcrete, LocationInverter, NativeAsset, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	UsingComponents, AsPrefixedGeneralIndex,
};
use xcm_executor::{traits::{JustTry, ShouldExecute}, XcmExecutor};

parameter_types! {
	pub const RelayLocation: MultiLocation = MultiLocation::parent();
	pub const LocalLocation: MultiLocation = MultiLocation::here();
	pub const RelayNetwork: NetworkId = NetworkId::Any;
	pub RelayChainOrigin: Origin = cumulus_pallet_xcm::Origin::Relay.into();
	pub Ancestry: MultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// Means for transacting the native currency on this chain.
pub type CurrencyTransactor = CurrencyAdapter<
	// Use this currency:
	Balances,
	// Use this currency when it is a fungible asset matching the given location or name:
	IsConcrete<LocalLocation>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	CheckingAccount,
>;

pub type IdConversion = (
	AsPrefixedGeneralIndex<AssetsPalletLocation, AssetId, JustTry>,
	AsPrefixedGeneralIndex<AssetsPalletLocation2K, AssetId, JustTry>,
	AsPrefixedGeneralIndex<AssetsPalletLocation3K, AssetId, JustTry>,
);

/// Means for transacting assets besides the native currency on this chain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation:
	Assets,
	// Use this currency when it is a fungible asset matching the given location or name:
	ConvertedConcreteAssetId<
		AssetId,
		Balance,
		IdConversion,
		JustTry,
	>,
	// Convert an XCM MultiLocation into a local account id:
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly):
	AccountId,
	// We only want to allow teleports of known assets. We use non-zero issuance as an indication
	// that this asset is known.
	parachains_common::impls::NonZeroIssuance<AccountId, Assets>,
	// The account to use for tracking teleports.
	CheckingAccount,
>;
/// Means for transacting assets on this chain.
pub type AssetTransactors = (CurrencyTransactor, FungiblesTransactor);

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, Origin>,
	// Native converter for Relay-chain (Parent) location; will converts to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, Origin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, Origin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, Origin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<Origin>,
);

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: u64 = 1_000_000_000;
	pub const MaxInstructions: u32 = 100;
}

match_types! {
	pub type ParentOrParentsExecutivePlurality: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(Plurality { id: BodyId::Executive, .. }) }
	};
}

//TODO: move DenyThenTry to polkadot's xcm module.
/// Deny executing the xcm message if it matches any of the Deny filter regardless of anything else.
/// If it passes the Deny, and matches one of the Allow cases then it is let through.
pub struct DenyThenTry<Deny, Allow>(PhantomData<Deny>, PhantomData<Allow>)
where
	Deny: ShouldExecute,
	Allow: ShouldExecute;

impl<Deny, Allow> ShouldExecute for DenyThenTry<Deny, Allow>
where
	Deny: ShouldExecute,
	Allow: ShouldExecute,
{
	fn should_execute<Call>(
		origin: &MultiLocation,
		message: &mut Xcm<Call>,
		max_weight: XCMWeight,
		weight_credit: &mut XCMWeight,
	) -> Result<(), ()> {
		Deny::should_execute(origin, message, max_weight, weight_credit)?;
		Allow::should_execute(origin, message, max_weight, weight_credit)
	}
}

// See issue #5233
pub struct DenyReserveTransferToRelayChain;
impl ShouldExecute for DenyReserveTransferToRelayChain {
	fn should_execute<Call>(
		origin: &MultiLocation,
		message: &mut Xcm<Call>,
		_max_weight: XCMWeight,
		_weight_credit: &mut XCMWeight,
	) -> Result<(), ()> {
		if message.0.iter().any(|inst| {
			matches!(
				inst,
				InitiateReserveWithdraw {
					reserve: MultiLocation { parents: 1, interior: Here },
					..
				} | DepositReserveAsset { dest: MultiLocation { parents: 1, interior: Here }, .. } |
					TransferReserveAsset {
						dest: MultiLocation { parents: 1, interior: Here },
						..
					}
			)
		}) {
			return Err(()) // Deny
		}

		// An unexpected reserve transfer has arrived from the Relay Chain. Generally, `IsReserve`
		// should not allow this, but we just log it here.
		if matches!(origin, MultiLocation { parents: 1, interior: Here }) &&
			message.0.iter().any(|inst| matches!(inst, ReserveAssetDeposited { .. }))
		{
			log::warn!(
				target: "xcm::barriers",
				"Unexpected ReserveAssetDeposited from the Relay Chain",
			);
		}
		// Permit everything else
		Ok(())
	}
}

pub type Barrier = DenyThenTry<
	DenyReserveTransferToRelayChain,
	(
		TakeWeightCredit,
		AllowTopLevelPaidExecutionFrom<Everything>,
		AllowUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
		// ^^^ Parent and its exec plurality get free execution
	),
>;

parameter_types! {
	pub Parachain2kLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(2000)));
	pub Parachain3kLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(3000)));
	// ALWAYS ensure that the index in PalletInstance stays up-to-date with
	// Statemint's Assets pallet index
	pub AssetsPalletLocation: MultiLocation =
		PalletInstance(<Assets as PalletInfoAccess>::index() as u8).into();
	pub AssetsPalletLocation2K: MultiLocation =
	MultiLocation::new(1, X2(Parachain(2000), PalletInstance(<Assets as PalletInfoAccess>::index() as u8)));
	pub AssetsPalletLocation3K: MultiLocation =
	MultiLocation::new(1, X2(Parachain(3000), PalletInstance(<Assets as PalletInfoAccess>::index() as u8)));
}

pub fn starts_with(loc: &MultiLocation, prefix: &MultiLocation) -> bool {
	if loc.parents != prefix.parents {
		return false
	}
	junctions_starts_with(&loc.interior, &prefix.interior)
}
pub fn junctions_starts_with(loc: &Junctions, prefix: &Junctions) -> bool {
	if loc.len() < prefix.len() {
		return false
	}
	prefix.iter().zip(loc.iter()).all(|(l, r)| l == r)
}

use xcm_executor::traits::FilterAssetLocation;
use frame_support::pallet_prelude::Get;
/// Asset filter that allows all assets from a certain location.
pub struct AssetsFrom<T>(PhantomData<T>);
impl<T: Get<MultiLocation>> FilterAssetLocation for AssetsFrom<T> {
	fn filter_asset_location(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		let loc = T::get();
		let allow = &loc == origin &&
			matches!(asset, MultiAsset { id: Concrete(asset_loc), fun: Fungible(_a) }
			if starts_with(asset_loc, &loc));
		log::debug!(target: "xcm::FilterAssetLocation", "AssetsFrom<{loc:?}>: allow: {allow:?} for asset: {asset:?} and origin: {origin:?}");
		allow
	}
}

pub type Reserves = (NativeAsset, AssetsFrom<Parachain2kLocation>, AssetsFrom<Parachain3kLocation>);

use xcm_executor::traits::WeightTrader;
pub struct NoopTrader;
impl WeightTrader for NoopTrader {
	/// Create a new trader instance.
	fn new() -> Self {
		Self
	}

	/// Purchase execution weight credit in return for up to a given `fee`. If less of the fee is required
	/// then the surplus is returned. If the `fee` cannot be used to pay for the `weight`, then an error is
	/// returned.
	fn buy_weight(&mut self, weight: u64, payment: xcm_executor::Assets) -> Result<xcm_executor::Assets, XcmError> {
		Ok(payment)
	}
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type Call = Call;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = AssetTransactors;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = Reserves;
	type IsTeleporter = (); // Teleporting is disabled.
	type LocationInverter = LocationInverter<Ancestry>;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call, MaxInstructions>;
	type Trader = NoopTrader;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
}

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<Origin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, ()>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
	type Event = Event;
	type SendXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	// ^ Disable dispatchable execute on the XCM pallet.
	// Needs to be `Everything` for local testing.
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Everything;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call, MaxInstructions>;
	type LocationInverter = LocationInverter<Ancestry>;
	type Origin = Origin;
	type Call = Call;

	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	// ^ Override for AdvertisedXcmVersion default
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
}

impl cumulus_pallet_xcm::Config for Runtime {
	type Event = Event;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}
