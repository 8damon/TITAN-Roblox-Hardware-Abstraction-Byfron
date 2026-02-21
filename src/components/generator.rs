use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Win32::Foundation::NTSTATUS;
use windows::Win32::Security::Cryptography::{
    BCRYPT_ALG_HANDLE, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS, BCRYPT_RNG_ALGORITHM,
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, BCryptOpenAlgorithmProvider,
    MS_PLATFORM_CRYPTO_PROVIDER,
};

struct EntropySource;

impl EntropySource {
    fn tpm_rng_handle() -> Option<BCRYPT_ALG_HANDLE> {
        static TPM_RNG_HANDLE: OnceLock<Option<usize>> = OnceLock::new();

        let raw = TPM_RNG_HANDLE.get_or_init(|| {
            // Kept process-lifetime by design; handle close at process teardown is unnecessary.
            // If TPM provider isn't available, we cache None and use BCrypt system fallback.
            let mut alg = BCRYPT_ALG_HANDLE::default();
            let status = unsafe {
                BCryptOpenAlgorithmProvider(
                    &mut alg,
                    BCRYPT_RNG_ALGORITHM,
                    MS_PLATFORM_CRYPTO_PROVIDER,
                    BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
                )
            };

            if nt_success(status) {
                Some(alg.0 as usize)
            } else {
                None
            }
        });

        raw.map(|h| BCRYPT_ALG_HANDLE(h as *mut c_void))
    }

    fn fill(out: &mut [u8]) {
        if let Some(alg) = Self::tpm_rng_handle() {
            let status = unsafe { BCryptGenRandom(Some(alg), out, Default::default()) };
            if nt_success(status) {
                return;
            }
        }

        let status = unsafe { BCryptGenRandom(None, out, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
        if !nt_success(status) {
            panic!("Secure RNG unavailable (BCryptGenRandom failed)");
        }
    }
}

fn nt_success(status: NTSTATUS) -> bool {
    status.0 >= 0
}

fn random_u32() -> u32 {
    let mut b = [0u8; 4];
    EntropySource::fill(&mut b);
    u32::from_le_bytes(b)
}

pub fn gen_guid() -> String {
    let mut buf = [0u8; 16];
    EntropySource::fill(&mut buf);
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        buf[0],
        buf[1],
        buf[2],
        buf[3],
        buf[4],
        buf[5],
        buf[6],
        buf[7],
        buf[8],
        buf[9],
        buf[10],
        buf[11],
        buf[12],
        buf[13],
        buf[14],
        buf[15]
    )
}

pub fn gen_serial() -> String {
    (0..10)
        .map(|_| (b'0' + (random_u32() % 10) as u8) as char)
        .collect()
}

pub fn gen_processor_id() -> String {
    format!("BFEBFBFF{:08X}", random_u32())
}

pub fn gen_pnp_id() -> String {
    let ven: String = (0..4)
        .map(|_| (b'A' + (random_u32() % 26) as u8) as char)
        .collect();
    let prod: String = (0..8)
        .map(|_| (b'0' + (random_u32() % 10) as u8) as char)
        .collect();
    let rev: String = (0..3)
        .map(|_| (b'0' + (random_u32() % 10) as u8) as char)
        .collect();

    format!(
        "SCSI\\DiskVen_{}&Prod_{}&Rev_{}\\4&{:08x}&0&0000",
        ven,
        prod,
        rev,
        random_u32()
    )
}

pub fn gen_device_id() -> String {
    format!("\\\\.\\PhysicalDrive{}", random_u32() % 10)
}

pub fn gen_users() -> String {
    let names = ["John", "Alice", "Bob", "Eve"];
    names[random_u32() as usize % names.len()].to_string()
}

pub fn gen_edid() -> String {
    let mut buf = [0u8; 8];
    EntropySource::fill(&mut buf);
    format!(
        "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]
    )
}
