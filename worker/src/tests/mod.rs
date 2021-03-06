/*
	Copyright 2019 Supercomputing Systems AG

	Licensed under the Apache License, Version 2.0 (the "License");
	you may not use this file except in compliance with the License.
	You may obtain a copy of the License at

		http://www.apache.org/licenses/LICENSE-2.0

	Unless required by applicable law or agreed to in writing, software
	distributed under the License is distributed on an "AS IS" BASIS,
	WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
	See the License for the specific language governing permissions and
	limitations under the License.

*/

use clap::ArgMatches;
use sgx_types::*;

use crate::enclave::api::*;
use crate::enclave::init::init_enclave;

use self::ecalls::*;
use self::integration_tests::*;

pub mod commons;
pub mod ecalls;
pub mod integration_tests;

pub fn run_enclave_tests(matches: &ArgMatches, port: &str) {
	println!("*** Starting Test enclave");
	let enclave = init_enclave().unwrap();

	if matches.is_present("all") || matches.is_present("unit") {
		println!("Running unit Tests");
		run_enclave_unit_tests(enclave.geteid());
	}

	if matches.is_present("all") || matches.is_present("ecall") {
		println!("Running ecall Tests");
		run_ecalls(enclave.geteid());
	}

	if matches.is_present("all") || matches.is_present("integration") {
		println!("Running integration Tests");
		run_integration_tests(enclave.geteid(), port);
	}
	println!("[+] All tests ended!");
}

fn run_enclave_unit_tests(eid: sgx_enclave_id_t) {

	let mut retval = 0usize;

	let result = unsafe {
		test_main_entrance(eid,
						   &mut retval)
	};

	match result {
		sgx_status_t::SGX_SUCCESS => {},
		_ => {
			println!("[-] ECALL Enclave Failed {}!", result.as_str());
			return;
		}
	}

	assert_eq!(retval, 0);
	println!("[+] unit_test ended!");
}



pub fn run_ecalls(eid: sgx_enclave_id_t) {
	println!("  testing execute_stf()");
	execute_stf_works(eid);
	println!("  testing get_state()");
	get_state_works(eid);


	println!("[+] Ecall tests ended!");
}

// Fixme: It is not nice to need to forward the port. Better: setup a node running on some port before
// running the tests.
pub fn run_integration_tests(eid: sgx_enclave_id_t, port: &str) {
	perform_ra_works(eid, port);
	process_forwarded_payload_works(eid, port);
}
