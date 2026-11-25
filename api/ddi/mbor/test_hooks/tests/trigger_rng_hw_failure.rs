// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! RNG continuous health-test (SP800-90B) fault injection, driven through the
//! opaque `TestAction` payload path.
//!
//! Set `RNG_HW_FAILURE_TEST_NUMBER` to the health test to fail: `0` = adaptive
//! proportion test (APT), `1` = repetition count test (RCT). The test is
//! skipped when it is not set and fails when it is set to an invalid value. Run each selection as a separate, serialized
//! invocation.
//!
//! Behaviour on this firmware:
//! - Requires a **physical** device; virtual/mock devices are skipped.
//! - Requires firmware built with `fips_validation_hooks`; otherwise the
//!   request returns [`DdiStatus::UnsupportedCmd`] and the case is skipped.
//! - The firmware tightens the selected hardware health-test cutoff. The RNG
//!   error interrupt fires and the HSM halts in the FIPS error state before it
//!   responds. As in the reference test, the request is asserted to succeed,
//!   so the test **fails** with the driver error (e.g. `IoAborted`). This
//!   explicitly reports that the device failed. A passing test means the alarm
//!   did not fire.
//! - As with the reference firmware, the device is intentionally left unusable
//!   after the hardware failure (the injected RNG configuration persists across
//!   warm resets). A power-on reset is required before running further tests,
//!   so post-test cleanup is best-effort and does not mask the failure.

#![allow(clippy::unwrap_used)]

mod common;

use std::env;
use std::error::Error;

use common::*;
use test_with_tracing::test;
use tsra_ddi::*;
use tsra_ddi_mbor_test_hooks::helper_test_action_cmd;
use tsra_ddi_mbor_test_hooks::TestActionRequest;
use tsra_ddi_mbor_types::*;

const REV: DdiApiRev = DdiApiRev { major: 1, minor: 0 };
const TEST_NUMBER_VAR: &str = "RNG_HW_FAILURE_TEST_NUMBER";
const MAX_RNG_HW_FAILURE_TEST_NUMBER: u32 = 1;

#[test]
fn trigger_rng_hw_failure() {
    // Validate before the harness so a skipped run never erases the device.
    let Ok(input) = env::var(TEST_NUMBER_VAR) else {
        println!("{TEST_NUMBER_VAR} not set; skipping.");
        return;
    };
    if let Err(err) = parse_rng_hw_failure_test_number(&input) {
        panic!("invalid {TEST_NUMBER_VAR}={input:?}: {err}");
    }

    ddi_dev_test(
        common_setup,
        rng_hw_failure_cleanup,
        |dev, _ddi, path, session_id| {
            let rng_test_id = read_rng_hw_failure_test_env_variable(TEST_NUMBER_VAR)
                .expect("RNG_HW_FAILURE_TEST_NUMBER validated before setup");

            let resp = helper_close_session(dev, Some(session_id), Some(REV));
            assert!(resp.is_ok(), "{:?}", resp);

            send_rng_hw_failure_request(path.to_string(), rng_test_id);
        },
    );
}

/// Best-effort cleanup: after the injected failure the device stays in the
/// FIPS error state until a power-on reset, so the erase is expected to fail.
fn rng_hw_failure_cleanup(
    dev: &mut <DdiTest as Ddi>::Dev,
    ddi: &DdiTest,
    path: &str,
    session_to_close: Option<u16>,
) {
    if let Some(session_id) = session_to_close {
        let _ = helper_close_session(dev, Some(session_id), Some(REV));
    }
    if let Ok(cleanup_dev) = ddi.open_dev(path) {
        if let Err(err) = cleanup_dev.erase() {
            println!("Cleanup erase failed as expected after RNG failure: {err:?}");
        }
    }
}

fn read_rng_hw_failure_test_env_variable(var_name: &str) -> Result<u32, Box<dyn Error>> {
    let env_var = env::var(var_name)?;
    parse_rng_hw_failure_test_number(&env_var)
}

fn parse_rng_hw_failure_test_number(input: &str) -> Result<u32, Box<dyn Error>> {
    let value: u32 = input.parse()?;
    if value > MAX_RNG_HW_FAILURE_TEST_NUMBER {
        return Err("invalid RNG health-test selector".into());
    }
    Ok(value)
}

#[test]
fn validate_rng_hw_failure_test_number() {
    assert_eq!(parse_rng_hw_failure_test_number("0").unwrap(), 0);
    assert_eq!(parse_rng_hw_failure_test_number("1").unwrap(), 1);
    assert!(parse_rng_hw_failure_test_number("2").is_err());
    assert!(parse_rng_hw_failure_test_number("-1").is_err());
    assert!(parse_rng_hw_failure_test_number("invalid").is_err());
}

fn send_rng_hw_failure_request(device_path: String, test: u32) {
    let ddi = DdiTest::default();
    let mut dev = ddi.open_dev(device_path.as_str()).unwrap();

    if get_device_kind(&mut dev) == DdiDeviceKind::Virtual {
        println!("RNG hardware failure is only supported on a physical device; skipping.");
        return;
    }

    let (encrypted_credential, pub_key) =
        encrypt_userid_pin_for_open_session(&dev, TEST_CRED_ID, TEST_CRED_PIN, TEST_SESSION_SEED);
    let resp = helper_open_session(&dev, None, Some(REV), encrypted_credential, pub_key);
    assert!(resp.is_ok(), "{:?}", resp);
    let session_id = resp.unwrap().hdr.sess_id.unwrap();

    let resp = helper_test_action_cmd(
        &mut dev,
        session_id,
        TestActionRequest::TriggerRngHwFailure(test),
    );
    if let Err(DdiError::DdiStatus(DdiStatus::UnsupportedCmd)) = &resp {
        println!("Firmware is not built with fips_validation_hooks.");
        return;
    }
    assert!(resp.is_ok(), "resp {:?}", resp);
}
