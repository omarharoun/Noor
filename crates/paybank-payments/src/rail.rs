use paybank_core::PaymentRail;

pub fn choose_rail(
    supports_fednow: bool,
    supports_rtp: bool,
    _supports_wire: bool,
    amount_cents: i64,
) -> PaymentRail {
    PaymentRail::choose(supports_fednow, supports_rtp, amount_cents)
}