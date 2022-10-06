#![cfg(test)]

use frame_support::{assert_noop, assert_ok};

use polkadot_xcm::{latest::prelude::*, VersionedMultiAssets};

use cumulus_primitives_core::ParaId;
use hex_literal::hex;
use num_traits::bounds::UpperBounded;
use sp_core::H256;
use sp_runtime::traits::{AccountIdConversion, BlakeTwo256, Hash};
use xcm_emulator::TestExt;

use frame_support::traits::{GenesisBuild, PalletInfoAccess};
use frame_support::pallet_prelude::Weight;
use polkadot_primitives::v2::{BlockNumber, MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use pretty_assertions::assert_eq;

use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain};

use para_runtime::{AccountId, Balance};

// ----------------------------------------------------------------
// Emulator Setup

pub const ALICE: [u8; 32] = [4u8; 32];
pub const BOB: [u8; 32] = [5u8; 32];
pub const CHARLIE: [u8; 32] = [6u8; 32];
pub const DAVE: [u8; 32] = [7u8; 32];

pub const UNITS: Balance = 1_000_000_000_000;

pub const BOB_INITIAL_BALANCE: u128 = 1000 * UNITS;

decl_test_relay_chain! {
	pub struct RococoRelay {
		Runtime = rococo_runtime::Runtime,
		XcmConfig = rococo_runtime::xcm_config::XcmConfig,
		new_ext = rococo_ext(),
	}
}

decl_test_parachain! {
	pub struct Para2k {
		Runtime = para_runtime::Runtime,
		Origin = para_runtime::Origin,
		XcmpMessageHandler = para_runtime::XcmpQueue,
		DmpMessageHandler = para_runtime::DmpQueue,
		new_ext = para_2k_ext(),
	}
}

decl_test_parachain! {
	pub struct Para3k {
		Runtime = para_runtime::Runtime,
		Origin = para_runtime::Origin,
		XcmpMessageHandler = para_runtime::XcmpQueue,
		DmpMessageHandler = para_runtime::DmpQueue,
		new_ext = para_3k_ext(),
	}
}

decl_test_network! {
	pub struct TestNet {
		relay_chain = RococoRelay,
		parachains = vec![
			(2000, Para2k),
			(3000, Para3k),
		],
	}
}

fn default_parachains_host_configuration() -> HostConfiguration<BlockNumber> {
	HostConfiguration {
		minimum_validation_upgrade_delay: 5,
		validation_upgrade_cooldown: 5u32,
		validation_upgrade_delay: 5,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
		group_rotation_frequency: 20,
		chain_availability_period: 4,
		thread_availability_period: 4,
		max_upward_queue_count: 8,
		max_upward_queue_size: 1024 * 1024,
		max_downward_message_size: 1024,
		ump_service_total_weight: Weight::from_ref_time(4 * 1_000_000_000),
		max_upward_message_size: 50 * 1024,
		max_upward_message_num_per_candidate: 5,
		hrmp_sender_deposit: 0,
		hrmp_recipient_deposit: 0,
		hrmp_channel_max_capacity: 8,
		hrmp_channel_max_total_size: 8 * 1024,
		hrmp_max_parachain_inbound_channels: 4,
		hrmp_max_parathread_inbound_channels: 4,
		hrmp_channel_max_message_size: 1024 * 1024,
		hrmp_max_parachain_outbound_channels: 4,
		hrmp_max_parathread_outbound_channels: 4,
		hrmp_max_message_num_per_candidate: 5,
		dispute_period: 6,
		no_show_slots: 2,
		n_delay_tranches: 25,
		needed_approvals: 2,
		relay_vrf_modulo_samples: 2,
		zeroth_delay_tranche_width: 0,
		..Default::default()
	}
}

pub fn rococo_ext() -> sp_io::TestExternalities {
	use rococo_runtime::{Runtime, System};

	let mut t = frame_system::GenesisConfig::default()
		.build_storage::<Runtime>()
		.unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(AccountId::from(ALICE), 2002 * UNITS),
			(ParaId::from(2000).into_account_truncating(), 10 * UNITS),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	polkadot_runtime_parachains::configuration::GenesisConfig::<Runtime> {
		config: default_parachains_host_configuration(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&pallet_xcm::GenesisConfig {
			safe_xcm_version: Some(2),
		},
		&mut t,
	)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn para_3k_ext() -> sp_io::TestExternalities {
	use para_runtime::{Runtime, System};

	let mut t = frame_system::GenesisConfig::default()
		.build_storage::<Runtime>()
		.unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(AccountId::from(ALICE), 200 * UNITS)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_assets::GenesisConfig::<Runtime> {
		assets: vec![(0, AccountId::from(ALICE), true, UNITS)],
		metadata: vec![(0, b"TEST".to_vec(), b"TestCoin".to_vec(), 3)],
		accounts: vec![(0, AccountId::from(ALICE), 2_000 * UNITS)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	<parachain_info::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&parachain_info::GenesisConfig {
			parachain_id: 3000.into(),
		},
		&mut t,
	)
	.unwrap();

	<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&pallet_xcm::GenesisConfig {
			safe_xcm_version: Some(2),
		},
		&mut t,
	)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn para_2k_ext() -> sp_io::TestExternalities {
	use para_runtime::{Runtime, System};
	use frame_support::traits::OnInitialize;

	let mut t = frame_system::GenesisConfig::default()
		.build_storage::<Runtime>()
		.unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(AccountId::from(ALICE), 200 * UNITS),
			(AccountId::from(BOB), BOB_INITIAL_BALANCE),
			(AccountId::from(CHARLIE), 1000 * UNITS),
			(AccountId::from(DAVE), 1000 * UNITS),
		],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_assets::GenesisConfig::<Runtime> {
		assets: vec![(0, AccountId::from(ALICE), true, UNITS)],
		metadata: vec![(0, b"TEST".to_vec(), b"TestCoin".to_vec(), 3)],
		accounts: vec![(0, AccountId::from(ALICE), 10 * UNITS)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	<parachain_info::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&parachain_info::GenesisConfig {
			parachain_id: 2000u32.into(),
		},
		&mut t,
	)
	.unwrap();

	<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&pallet_xcm::GenesisConfig {
			safe_xcm_version: Some(2),
		},
		&mut t,
	)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

fn last_events(n: usize) -> Vec<para_runtime::Event> {
	frame_system::Pallet::<para_runtime::Runtime>::events()
		.into_iter()
		.rev()
		.take(n)
		.rev()
		.map(|e| e.event)
		.collect()
}

pub fn expect_events(e: Vec<para_runtime::Event>) {
	assert_eq!(last_events(e.len()), e);
}

// Determine the hash for assets expected to be have been trapped.
fn determine_hash<M>(origin: &MultiLocation, assets: M) -> H256
where
	M: Into<MultiAssets>,
{
	let versioned = VersionedMultiAssets::from(assets.into());
	BlakeTwo256::hash_of(&(origin, &versioned))
}

// ----------------------------------------------------------------
// Tests

#[test]
fn transfer_from_2k_to_3k() {
	TestNet::reset();

	Para3k::execute_with(|| {
		env_logger::init();

		assert_ok!(para_runtime::PolkadotXcm::limited_reserve_transfer_assets(
			para_runtime::Origin::signed(ALICE.into()),
			Box::new(MultiLocation::new(1, X1(Junction::Parachain(2000))).into()),
			Box::new(
				MultiLocation::new(
					0,
					X1(
						Junction::AccountId32 {
							id: BOB,
							network: NetworkId::Any,
						}
					)
				)
				.into()
			),
			Box::new(MultiAssets::from((MultiLocation::new(0, X2(PalletInstance(<para_runtime::Assets as PalletInfoAccess>::index() as u8), GeneralIndex(0))), 300 * UNITS)).into()),
			0,
			Limited(399_600_000_000),
		));
		assert_eq!(
			para_runtime::Assets::balance(0, &AccountId::from(ALICE)),
			2_000 * UNITS - 300 * UNITS
		);
	});

	Para2k::execute_with(|| {
		assert_eq!(
			para_runtime::Assets::balance(0, &AccountId::from(BOB)),
			300 * UNITS
		);
	});
}
