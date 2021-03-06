use std::backtrace::{self, PrintFormat};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::str;
use std::sync::Arc;
use std::vec::Vec;

use sgx_types::*;

use rustls::{ClientSession, Stream};
use log::*;

use crate::attestation::create_ra_report_and_signature;
use crate::constants::ENCRYPTED_STATE_FILE;
use crate::cert;
use crate::{ocall_read_ipfs, ocall_write_ipfs};
use crate::io;
use crate::rsa3072;
use crate::aes;

struct ClientAuth {
	outdated_ok: bool,
}

impl ClientAuth {
	fn new(outdated_ok: bool) -> ClientAuth {
		ClientAuth { outdated_ok }
	}
}

impl rustls::ClientCertVerifier for ClientAuth {
	fn client_auth_root_subjects(&self) -> rustls::DistinguishedNames {
		rustls::DistinguishedNames::new()
	}

	fn verify_client_cert(&self, _certs: &[rustls::Certificate])
						  -> Result<rustls::ClientCertVerified, rustls::TLSError> {
		info!("client cert: {:?}", _certs);
		// This call will automatically verify cert is properly signed
		match cert::verify_mra_cert(&_certs[0].0) {
			Ok(()) => Ok(rustls::ClientCertVerified::assertion()),
			Err(sgx_status_t::SGX_ERROR_UPDATE_NEEDED) => {
				if self.outdated_ok {
					warn!("outdated_ok is set, overriding outdated error");
					Ok(rustls::ClientCertVerified::assertion())
				} else {
					Err(rustls::TLSError::WebPKIError(webpki::Error::ExtensionValueInvalid))
				}
			}
			Err(_) => {
				Err(rustls::TLSError::WebPKIError(webpki::Error::ExtensionValueInvalid))
			}
		}
	}
}

struct ServerAuth {
	outdated_ok: bool
}

impl ServerAuth {
	fn new(outdated_ok: bool) -> ServerAuth {
		ServerAuth { outdated_ok }
	}
}

impl rustls::ServerCertVerifier for ServerAuth {
	fn verify_server_cert(&self,
						  _roots: &rustls::RootCertStore,
						  _certs: &[rustls::Certificate],
						  _hostname: webpki::DNSNameRef,
						  _ocsp: &[u8]) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
		info!("server cert: {:?}", _certs);
		// This call will automatically verify cert is properly signed
		match cert::verify_mra_cert(&_certs[0].0) {
			Ok(()) => {
				Ok(rustls::ServerCertVerified::assertion())
			}
			Err(sgx_status_t::SGX_ERROR_UPDATE_NEEDED) => {
				if self.outdated_ok {
					warn!("outdated_ok is set, overriding outdated error");
					Ok(rustls::ServerCertVerified::assertion())
				} else {
					Err(rustls::TLSError::WebPKIError(webpki::Error::ExtensionValueInvalid))
				}
			}
			Err(_) => {
				Err(rustls::TLSError::WebPKIError(webpki::Error::ExtensionValueInvalid))
			}
		}
	}
}


#[no_mangle]
pub unsafe extern "C" fn run_server(socket_fd: c_int, sign_type: sgx_quote_sign_type_t) -> sgx_status_t {
	let _ = backtrace::enable_backtrace("enclave.signed.so", PrintFormat::Short);

	let (key_der, cert_der) = match create_ra_report_and_signature(sign_type) {
		Ok(r) => r,
		Err(e) => return e,
	};

	let mut cfg = rustls::ServerConfig::new(Arc::new(ClientAuth::new(true)));
	let mut certs = Vec::new();
	certs.push(rustls::Certificate(cert_der));
	let privkey = rustls::PrivateKey(key_der);

	cfg.set_single_cert_with_ocsp_and_sct(certs, privkey, vec![], vec![]).unwrap();

	let mut sess = rustls::ServerSession::new(&Arc::new(cfg));
	let mut conn = TcpStream::new(socket_fd).unwrap();
	let mut tls = rustls::Stream::new(&mut sess, &mut conn);

	println!("    [Enclave] (MU-RA-Server) MU-RA successful sending keys");

	let shielding_key = rsa3072::unseal_pair().unwrap();
	let (key, iv) = aes::read_or_create_sealed().unwrap();
	let rsa_pair = serde_json::to_string(&shielding_key).unwrap();
	let rsa_len = rsa_pair.as_bytes().len();
	info!("Sending Shielding Key: {:?}", rsa_len);
	info!("Sending AES key {:?}\nIV: {:?}\n", key, iv);

	tls.write(&rsa_len.to_le_bytes()).unwrap();
	tls.write(rsa_pair.as_bytes()).unwrap();
	tls.write(&key[..]).unwrap();
	tls.write(&iv[..]).unwrap();

	println!("    [Enclave] (MU-RA-Server) Keys sent, writing state to IPFS (= file hosting service)");

	let enc_state = match io::read_plaintext(ENCRYPTED_STATE_FILE) {
		Ok(state) => state,
		Err(status) => return status,
	};

	info!("Sending encrypted state length");
	tls.write(&enc_state.len().to_le_bytes()).unwrap();

	if enc_state.is_empty() {
		println!("    [Enclave] (MU-RA-Server) No state has been written yet. Nothing to write to ipfs.");
		println!("    [Enclave] (MU-RA-Server) Registration procedure successful!\n");
	}

	let mut rt: sgx_status_t = sgx_status_t::SGX_ERROR_UNEXPECTED;
	let mut cid_buf: [u8; 46] = [0; 46];
	let res = ocall_write_ipfs(&mut rt as *mut sgx_status_t,
							   enc_state.as_ptr() as *const u8,
							   enc_state.len() as u32,
							   cid_buf.as_mut_ptr() as *mut u8,
							   cid_buf.len() as u32);

	if res == sgx_status_t::SGX_ERROR_UNEXPECTED || rt == sgx_status_t::SGX_ERROR_UNEXPECTED {
		return sgx_status_t::SGX_ERROR_UNEXPECTED;
	}

	println!("    [Enclave] (MU-RA-Server) Write to IPFS successful, sending storage hash");
	tls.write(&cid_buf).unwrap();
	println!("    [Enclave] (MU-RA-Server) Registration procedure successful!\n");
	sgx_status_t::SGX_SUCCESS
}

#[no_mangle]
pub extern "C" fn run_client(socket_fd: c_int, sign_type: sgx_quote_sign_type_t) -> sgx_status_t {
	let _ = backtrace::enable_backtrace("enclave.signed.so", PrintFormat::Short);

	let (key_der, cert_der) = match create_ra_report_and_signature(sign_type) {
		Ok(r) => r,
		Err(e) => return e,
	};

	let mut cfg = rustls::ClientConfig::new();
	let mut certs = Vec::new();
	certs.push(rustls::Certificate(cert_der));
	let privkey = rustls::PrivateKey(key_der);

	cfg.set_single_client_cert(certs, privkey);
	cfg.dangerous().set_certificate_verifier(Arc::new(ServerAuth::new(true)));
	cfg.versions.clear();
	cfg.versions.push(rustls::ProtocolVersion::TLSv1_2);

	let dns_name = webpki::DNSNameRef::try_from_ascii_str("localhost").unwrap();
	let mut sess = rustls::ClientSession::new(&Arc::new(cfg), dns_name);
	let mut conn = TcpStream::new(socket_fd).unwrap();

	let mut tls = rustls::Stream::new(&mut sess, &mut conn);

	println!();
	println!("    [Enclave] (MU-RA-Client) MU-RA successful waiting for keys...");

	let mut key_len_arr = [0u8; 8];
	let key_len = match tls.read(&mut key_len_arr) {
		Ok(_) => {
			info!("Received rsa key_len: {:?}", &key_len_arr.to_vec());
			usize::from_le_bytes(key_len_arr)
		}
		Err(e) => {
			error!("Error in read: {:?}", e);
			return sgx_status_t::SGX_ERROR_UNEXPECTED;
		}
	};

	let mut rsa_pair = vec![0u8; key_len]; //Vec::new();
	match tls.read(&mut rsa_pair) {
		Ok(_) => info!("Received Shielding key: {}", str::from_utf8(&rsa_pair).unwrap()),
		Err(e) => {
			error!("Error in read: {:?}", e);
			return sgx_status_t::SGX_ERROR_UNEXPECTED;
		}
	};
	if let Err(e) = rsa3072::seal(&rsa_pair) {
		return e;
	}

	let mut aes_key = [0u8; 16]; //Vec::new();
	match tls.read(&mut aes_key) {
		Ok(_) => info!("Received AES key: {:?}", &aes_key[..]),
		Err(e) => {
			error!("Error in read: {:?}", e);
			return sgx_status_t::SGX_ERROR_UNEXPECTED
		}
	};

	let mut aes_iv = [0u8; 16]; //Vec::new();
	match tls.read(&mut aes_iv) {
		Ok(_) => info!("Received AES IV: {:?}", &aes_iv[..]),
		Err(e) => {
			error!("Error in read: {:?}", e);
			return sgx_status_t::SGX_ERROR_UNEXPECTED
		}
	};

	if let Err(e) = aes::seal(aes_key, aes_iv) {
		return e;
	}

	println!("    [Enclave] (MU-RA-Client) Received and stored keys, waiting for storage hash...");

	let mut state_len_arr = [0u8; 8];
	let state_len = match tls.read(&mut state_len_arr) {
		Ok(_) => {
			info!("Received enc_state_len: {:?}", &state_len_arr.to_vec());
			usize::from_le_bytes(state_len_arr)
		}
		Err(e) => {
			error!("Error in read: {:?}", e);
			return sgx_status_t::SGX_ERROR_UNEXPECTED;
		}
	};

	if state_len == 0 {
		println!("    [Enclave] (MU-RA-Client) No state has been written yet, nothing to fetch from IPFS");
		println!("    [Enclave] (MU-RA-Client) Registration Procedure successful!\n");
		return sgx_status_t::SGX_SUCCESS;
	}

	let mut cid = [0u8; 46];
	match tls.read(&mut cid) {
		Ok(_) => info!("Received ipfs CID: {:?}", &cid[..]),
		Err(e) => {
			error!("Error in read: {:?}", e);
			return sgx_status_t::SGX_ERROR_UNEXPECTED
		}
	};

	println!("    [Enclave] (MU-RA-Client) Received IPFS storage hash, reading from IPFS...");

	let mut enc_state = vec![0u8; state_len];
	let mut rt: sgx_status_t = sgx_status_t::SGX_ERROR_UNEXPECTED;
	let _res = unsafe {
		ocall_read_ipfs(&mut rt as *mut sgx_status_t,
						enc_state.as_mut_ptr(),
						enc_state.len() as u32,
						cid.as_ptr(),
						cid.len() as u32)
	};



	println!("    [Enclave] (MU-RA-Client) Got encrypted state from ipfs: {:?}\n", enc_state);
	if let Err(e) = io::write_plaintext(&enc_state, ENCRYPTED_STATE_FILE) {
		return e;
	}

	println!("    [Enclave] (MU-RA-Client) Successfully read state from IPFS");
	println!("    [Enclave] (MU-RA-Client) Registration procedure successful!\n");

	sgx_status_t::SGX_SUCCESS
}

#[allow(dead_code)]
fn read_tls_stream(tls: &mut Stream<ClientSession, TcpStream>, buff: &mut Vec<u8>, msg: &str) -> SgxResult<Vec<u8>> {
	match tls.read(buff) {
		Ok(_) => {
			println!("{}: {:?}", msg, &buff[..]);
			Ok(buff.to_vec())
		},
		Err(e) => {
			println!("Error in read: {:?}", e);
			Err(sgx_status_t::SGX_ERROR_UNEXPECTED)
		}
	}
}
