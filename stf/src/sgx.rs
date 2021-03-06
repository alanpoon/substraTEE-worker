use sgx_tstd as std;
use std::prelude::v1::*;

use codec::Encode;
use log::*;
use primitives::hashing::{blake2_256, twox_128};
use runtime_primitives::traits::Dispatchable;

use sgx_runtime::Runtime;
use sr_io::SgxExternalitiesTrait;

use crate::{State, Stf, TrustedCall, TrustedGetter};

#[cfg(feature = "sgx")]
impl Stf {
	pub fn init_state() -> State {
		debug!("initializing stf state");
		let mut ext = State::new();
		ext.execute_with(|| {
			sr_io::storage::set(&storage_key_bytes("Balances", "TotalIssuance", None), &11u128.encode());
			sr_io::storage::set(&storage_key_bytes("Balances", "CreationFee", None), &1u128.encode());
			sr_io::storage::set(&storage_key_bytes("Balances", "TransferFee", None), &1u128.encode());
			sr_io::storage::set(&storage_key_bytes("Balances", "TransactionBaseFee", None), &1u128.encode());
			sr_io::storage::set(&storage_key_bytes("Balances", "TransfactionByteFee", None), &1u128.encode());
			sr_io::storage::set(&storage_key_bytes("Balances", "ExistentialDeposit", None), &1u128.encode());
		});
		ext
	}
	pub fn execute(ext: &mut State, call: TrustedCall) {
		ext.execute_with(|| {
			let _result = match call {
				TrustedCall::balance_set_balance(who, free_balance, reserved_balance) =>
					sgx_runtime::balancesCall::<Runtime>::set_balance(indices::Address::<Runtime>::Id(who.clone()), free_balance, reserved_balance).dispatch(sgx_runtime::Origin::ROOT),
				TrustedCall::balance_transfer(from, to, value) => {
					//FIXME: here would be a good place to really verify a signature
					let origin = sgx_runtime::Origin::signed(from.clone());
					sgx_runtime::balancesCall::<Runtime>::transfer(indices::Address::<Runtime>::Id(to.clone()), value).dispatch(origin)
				},
			};
		});
	}

	pub fn get_state(ext: &mut State, getter: TrustedGetter) -> Option<Vec<u8>> {
		//FIXME: only account owner may get its own data. introduce signature verification!
		ext.execute_with(|| {
			let result = match getter {
				TrustedGetter::free_balance(who) =>
					sr_io::storage::get(&storage_key_bytes("Balances", "FreeBalance", Some(who.encode()))),
				TrustedGetter::reserved_balance(who) =>
					sr_io::storage::get(&storage_key_bytes("Balances", "ReservedBalance", Some(who.encode()))),
			};
			debug!("get_state result: {:?}", result);
			result
		})
	}
}

pub fn storage_key_bytes(module: &str, storage_key_name: &str, param: Option<Vec<u8>>) -> Vec<u8> {
	let mut key = [module, storage_key_name].join(" ").as_bytes().to_vec();
	let keyhash;
	debug!("storage_key_hash for: module: {} key: {} (and params?) ", module, storage_key_name);
	match param {
		Some(par) => {
			key.append(&mut par.clone());
			keyhash = blake2_256(&key).to_vec();
		},
		_ => {
			keyhash = twox_128(&key).to_vec();
		},
	}
	//debug!("   is 0x{}", hex::encode_hex(&keyhash));
	keyhash
}

/*
pub fn init_runtime() {
	info!("[??] asking runtime out");

	let mut ext = SgxExternalities::new();

	let tina = AccountId::default();
	let origin_tina = sgx_runtime::Origin::signed(tina.clone());
	//let origin = sgx_runtime::Origin::ROOT;

	let address = indices::Address::<Runtime>::default();

	sr_io::with_externalities(&mut ext, || {
		// write Genesis
		info!("Prepare some Genesis values");
		sr_io::set_storage(&storage_key_bytes("Balances", "TotalIssuance", None), &11u128.encode());
		sr_io::set_storage(&storage_key_bytes("Balances", "CreationFee", None), &1u128.encode());
		sr_io::set_storage(&storage_key_bytes("Balances", "TransferFee", None), &1u128.encode());
		sr_io::set_storage(&storage_key_bytes("Balances", "TransactionBaseFee", None), &1u128.encode());
		sr_io::set_storage(&storage_key_bytes("Balances", "TransfactionByteFee", None), &1u128.encode());
		sr_io::set_storage(&storage_key_bytes("Balances", "ExistentialDeposit", None), &1u128.encode());
		// prefund Tina
		sr_io::set_storage(&storage_key_bytes("Balances", "FreeBalance", Some(tina.clone().encode())), & 13u128.encode());

		// read storage
		let _creation_fee = sr_io::storage(&storage_key_bytes("Balances", "ExistentialDeposit", None));
		debug!("reading genesis storage ExistentialDeposit = {:?}", _creation_fee);

		const MILLICENTS: u128 = 1_000_000_000;
		const CENTS: u128 = 1_000 * MILLICENTS;    // assume this is worth about a cent.

		info!("re-funding tina: call set_balance");
		let res = sgx_runtime::balancesCall::<Runtime>::set_balance(indices::Address::<Runtime>::Id(tina.clone()), 42, 43).dispatch(sgx_runtime::Origin::ROOT);
		info!("reading Tina's FreeBalance");
		let tina_balance = sr_io::storage(&storage_key_bytes("Balances", "FreeBalance", Some(tina.clone().encode())));
		info!("Tina's FreeBalance is {:?}", tina_balance);
	});
	info!("[++] finished playing with runtime");
}
*/
