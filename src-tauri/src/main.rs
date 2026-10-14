fn main() {
    #[cfg(feature = "desktop")]
    if let Err(error) = fix_path_env::fix() {
        eprintln!("Could not load the user's shell PATH: {error}");
    }
    handy_lib::run();
}
