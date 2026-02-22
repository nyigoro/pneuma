#[cfg(feature = "ladybird")]
extern "C" {
    fn pneuma_ladybird_sanity_check() -> i32;
}

#[cfg(feature = "ladybird")]
pub fn sanity_check() -> i32 {
    unsafe { pneuma_ladybird_sanity_check() }
}

#[cfg(all(test, feature = "ladybird"))]
mod tests {
    use super::sanity_check;

    #[test]
    fn abi_sanity_check_returns_sentinel() {
        assert_eq!(sanity_check(), 0xCAFE);
    }
}
