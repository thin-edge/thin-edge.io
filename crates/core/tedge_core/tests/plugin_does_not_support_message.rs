use futures::future::FutureExt;
use tedge_api::error::DirectoryError;
use tedge_api::error::PluginError;
use tedge_core::configuration::TedgeConfiguration;
use tedge_core::errors::TedgeApplicationError;
use tedge_core::TedgeApplication;

mod not_supported {
    use async_trait::async_trait;
    use tedge_api::plugin::PluginExt;
    use tedge_api::Plugin;
    use tedge_api::PluginBuilder;
    use tedge_api::PluginConfiguration;
    use tedge_api::PluginDirectory;
    use tedge_api::PluginError;

    pub const NOT_SUPPORTED_PLUGIN_NAME: &'static str = "not_supported";

    pub struct NotSupportedPluginBuilder;

    #[async_trait::async_trait]
    impl<PD: PluginDirectory> PluginBuilder<PD> for NotSupportedPluginBuilder {
        fn kind_name() -> &'static str {
            "notsupported"
        }

        async fn verify_configuration(
            &self,
            _config: &PluginConfiguration,
        ) -> Result<(), tedge_api::error::PluginError> {
            Ok(())
        }

        async fn instantiate(
            &self,
            _config: PluginConfiguration,
            _cancellation_token: tedge_api::CancellationToken,
            _plugin_dir: &PD,
        ) -> Result<tedge_api::plugin::BuiltPlugin, PluginError> {
            Ok(NotSupportedPlugin {}.into_untyped::<()>())
        }

        fn kind_message_types() -> tedge_api::plugin::HandleTypes
        where
            Self: Sized,
        {
            tedge_api::plugin::HandleTypes::declare_handlers_for::<(), NotSupportedPlugin>()
        }
    }

    struct NotSupportedPlugin;

    #[async_trait]
    impl Plugin for NotSupportedPlugin {
        async fn setup(&mut self) -> Result<(), PluginError> {
            tracing::info!("Setup called");
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), PluginError> {
            tracing::info!("Shutdown called");
            Ok(())
        }
    }
}

mod sending {
    use async_trait::async_trait;
    use tedge_api::plugin::PluginExt;
    use tedge_api::Plugin;
    use tedge_api::PluginBuilder;
    use tedge_api::PluginConfiguration;
    use tedge_api::PluginDirectory;
    use tedge_api::PluginError;

    pub struct SendingPluginBuilder;

    #[async_trait::async_trait]
    impl<PD: PluginDirectory> PluginBuilder<PD> for SendingPluginBuilder {
        fn kind_name() -> &'static str {
            "sending"
        }

        async fn verify_configuration(
            &self,
            _config: &PluginConfiguration,
        ) -> Result<(), tedge_api::error::PluginError> {
            Ok(())
        }

        async fn instantiate(
            &self,
            _config: PluginConfiguration,
            _cancellation_token: tedge_api::CancellationToken,
            plugin_dir: &PD,
        ) -> Result<tedge_api::plugin::BuiltPlugin, PluginError> {
            tracing::warn!("Going to fetch addresses that do not support the messages I expect");
            // this should not work
            let _target_addr = plugin_dir.get_address_for::<SendingMessages>(
                crate::not_supported::NOT_SUPPORTED_PLUGIN_NAME,
            )?;
            Ok(SendingPlugin {}.into_untyped::<()>())
        }

        fn kind_message_types() -> tedge_api::plugin::HandleTypes
        where
            Self: Sized,
        {
            tedge_api::plugin::HandleTypes::declare_handlers_for::<(), SendingPlugin>()
        }
    }

    struct SendingPlugin;

    #[async_trait]
    impl Plugin for SendingPlugin {
        async fn setup(&mut self) -> Result<(), PluginError> {
            tracing::info!("Setup called");
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), PluginError> {
            tracing::info!("Shutdown called");
            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct SendingMessage;
    impl tedge_api::plugin::Message for SendingMessage {
        type Reply = tedge_api::message::NoReply;
    }

    tedge_api::make_receiver_bundle!(pub struct SendingMessages(SendingMessage));
}

#[test_log::test(tokio::test)]
async fn test_not_supported_message() -> Result<(), Box<(dyn std::error::Error + 'static)>> {
    let _ = tracing_subscriber::fmt::try_init();

    let conf = format!(
        r#"
        communication_buffer_size = 10

        plugin_shutdown_timeout_ms = 2000

        [plugins]
        [plugins.{not_supported_plugin_name}]
        kind = "notsupported"
        [plugins.{not_supported_plugin_name}.configuration]

        [plugins.sender]
        kind = "sending"
        [plugins.sender.configuration]
    "#,
        not_supported_plugin_name = crate::not_supported::NOT_SUPPORTED_PLUGIN_NAME
    );

    let config: TedgeConfiguration = toml::de::from_str(&conf)?;
    let (cancel_sender, application) = TedgeApplication::builder()
        .with_plugin_builder(crate::not_supported::NotSupportedPluginBuilder {})?
        .with_plugin_builder(crate::sending::SendingPluginBuilder {})?
        .with_config(config)?;

    let run_fut = application.run();

    // send a cancel request to the app after 1 sec
    let cancel_fut = Box::pin({
        tokio::time::sleep(std::time::Duration::from_secs(1)).then(|_| async {
            tracing::info!("Cancelling app now");
            cancel_sender.cancel_app()
        })
    });

    tokio::select! {
        app_res = run_fut => {
            tracing::info!("application.run() returned");
            match app_res {
                Ok(_) => panic!("Application exited successfully. It should return an error though"),
                Err(e) => {
                    match e {
                        TedgeApplicationError::Plugin(PluginError::DirectoryError(DirectoryError::PluginDoesNotSupport(pl, supp))) => {
                            assert_eq!(pl, crate::not_supported::NOT_SUPPORTED_PLUGIN_NAME,
                                "Expected plugin which does not support messages to be named {}", crate::not_supported::NOT_SUPPORTED_PLUGIN_NAME);

                            assert_eq!(supp, ["plugin_does_not_support_message::sending::SendingMessage"],
                                "Expected not-supported-message to be 'plugin_does_not_support_message::sending::SendingMessage'");

                            Ok(())
                        }

                        other => {
                            panic!("Expected PluginDoesNotSupport error, found: {:?}", other);
                        }
                    }
                },
            }
        },

        _ = cancel_fut => {
            tracing::info!("Cancellation happened...");
            panic!("App should have exited on its own, but cancellation was necessary");
        }
    }
}
