fn set_command_fails_given_device_id() {
    let command = SetConfigCommand {
        config_key: ConfigKey::from_str("device.id"),
        // ...
    };

    let context = TestContext {}; // We would actually pass in an &dyn ExecutionContext, so to simplify testing.
    let result = command.execute(&context);

    assert_matches!(result, Err(ConfigSettingError::ReadonlySetting {..}));
}

// ...
