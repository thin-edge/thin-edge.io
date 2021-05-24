pub mod agent;
pub mod message;
pub mod plugin;
pub mod plugin_manager;
pub mod software;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
