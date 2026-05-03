#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    #[test]
    fn prandom_known_output() {
        setup();
        unsafe {
            random_seed = 0;
            seed_was_init = 1;
            let result = prandom(15);
            assert_eq!(result, 6);
        }
    }
}
