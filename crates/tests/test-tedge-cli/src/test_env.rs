pub struct C8YTestEnv {
    pub url: String,
    pub user: String,
    pub pass: String,
}

impl Default for C8YTestEnv {
    fn default() -> Self {
        let url = envvar("C8YURL", "the endpoint to connect c8y");
        let user = envvar("C8YUSERNAME", "the user to connect c8y");
        let pass = envvar("C8YPASSWORD", "the password to connect c8y");

        C8YTestEnv { url, user, pass }
    }
}

fn envvar(var: &str, doc: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("The {var} env variable must be set with {doc}"))
}
