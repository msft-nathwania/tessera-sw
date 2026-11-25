// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! TestAction 13: force an RNG continuous health-test failure.
//!
//! Selector `0` lowers the APT cutoff and `1` lowers the RCT cutoff, as in
//! the reference firmware. The hardware then raises the RNG error interrupt,
//! whose fatal handler enters the error state. Any other selector is
//! rejected before the RNG is touched.
//!
//! As in the reference, the injection is applied inline. The alarm halts
//! the core before the success response is posted, so the host sees the
//! command fail. A response that does arrive means the alarm did not fire.

use tsra_fw_ddi_mbor::MborDecoder;
use tsra_fw_ddi_mbor_derive::Ddi;
use tsra_fw_hsm_pal_traits::HsmDmaBuf;
use tsra_fw_hsm_pal_traits::HsmResult;
use tsra_fw_uno_drivers_rng::RngHealthTest;

use super::common::ReqHdr;
use super::test_action;
use crate::UnoHsmIo;
use crate::pal::UnoHsmPal;

/// Action-specific `TriggerRngHwFailure` request data.
#[derive(Ddi)]
#[ddi(map)]
struct TriggerRngHwFailureReq {
    /// Runtime health-test selector: 0 = APT, 1 = RCT.
    #[ddi(id = 1)]
    neg_test_id: u32,
}

pub(super) fn dispatch<'p>(
    pal: &'p UnoHsmPal,
    io: &UnoHsmIo,
    hdr: &ReqHdr,
    decoder: &mut MborDecoder<'_>,
    field_count: u8,
    request_len: usize,
) -> HsmResult<&'p HsmDmaBuf> {
    let request: TriggerRngHwFailureReq =
        test_action::decode_payload(decoder, field_count, request_len)?;
    let test = RngHealthTest::from_runtime_selector(request.neg_test_id)?;
    pal.rng.inject_hw_failure(test);
    test_action::encode_success(pal, io, hdr)
}
