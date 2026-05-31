use paybank_core::PaymentRail;

pub fn choose_rail(
    supports_fednow: bool,
    supports_rtp: bool,
    _supports_wire: bool,
    amount_cents: i64,
) -> PaymentRail {
    PaymentRail::choose(supports_fednow, supports_rtp, amount_cents)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Signature: choose_rail(supports_fednow, supports_rtp, supports_wire, amount_cents)

    #[test]
    fn above_threshold_chooses_wire_even_with_realtime_support() {
        // 2_500_001 cents is strictly greater than the $25,000 (2_500_000) cap.
        assert_eq!(choose_rail(true, true, true, 2_500_001), PaymentRail::Wire);
        assert_eq!(choose_rail(false, false, false, 2_500_001), PaymentRail::Wire);
        assert_eq!(choose_rail(false, true, true, 100_000_000), PaymentRail::Wire);
    }

    #[test]
    fn fednow_preferred_when_supported_below_threshold() {
        assert_eq!(choose_rail(true, true, true, 1_000_000), PaymentRail::FedNow);
        assert_eq!(choose_rail(true, false, false, 1), PaymentRail::FedNow);
    }

    #[test]
    fn rtp_chosen_when_no_fednow_but_rtp_supported() {
        assert_eq!(choose_rail(false, true, false, 500_000), PaymentRail::Rtp);
    }

    #[test]
    fn ach_is_fallback_when_no_realtime_support() {
        assert_eq!(choose_rail(false, false, false, 500_000), PaymentRail::Ach);
        assert_eq!(choose_rail(false, false, false, 0), PaymentRail::Ach);
    }

    #[test]
    fn boundary_at_exactly_threshold_is_not_wire() {
        // Exactly 2_500_000 is NOT > 2_500_000, so it routes by support flags.
        assert_eq!(choose_rail(true, true, true, 2_500_000), PaymentRail::FedNow);
        assert_eq!(choose_rail(false, true, true, 2_500_000), PaymentRail::Rtp);
        assert_eq!(choose_rail(false, false, true, 2_500_000), PaymentRail::Ach);
    }

    #[test]
    fn supports_wire_flag_does_not_change_below_threshold_routing() {
        // The wire flag does not affect routing; amount/fednow/rtp drive it.
        assert_eq!(choose_rail(false, false, true, 500_000), PaymentRail::Ach);
        assert_eq!(choose_rail(false, false, false, 500_000), PaymentRail::Ach);
        assert_eq!(choose_rail(true, false, true, 500_000), PaymentRail::FedNow);
    }
}