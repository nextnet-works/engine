use crate::helpers;
use crate::helpers::common::Infrastructure;
use crate::helpers::environment::session_is_sticky;
use crate::helpers::scaleway::clean_environments;
use crate::helpers::scaleway::scw_default_infra_config;
use crate::helpers::utilities::{context_for_resource, engine_run_test, get_pods, init, logger, FuncTestsSecrets};
use crate::helpers::utilities::{get_pvc, is_pod_restarted_env};
use ::function_name::named;
use bstr::ByteSlice;
use qovery_engine::cloud_provider::Kind;
use qovery_engine::cmd::kubectl::kubectl_get_secret;
use qovery_engine::io_models::application::{Port, Protocol, Storage, StorageType};

use qovery_engine::io_models::container::{Container, Registry};
use qovery_engine::io_models::context::CloneForTest;
use qovery_engine::io_models::job::{Job, JobSchedule, JobSource};
use qovery_engine::io_models::router::{Route, Router};
use qovery_engine::io_models::{Action, MountedFile, QoveryIdentifier};
use qovery_engine::models::scaleway::ScwZone;
use qovery_engine::transaction::TransactionResult;
use qovery_engine::utilities::to_short_id;
use retry::delay::Fibonacci;
use std::collections::BTreeMap;
use std::str::FromStr;
use tracing::{span, warn, Level};
use url::Url;
use uuid::Uuid;
// Note: All those tests relies on a test cluster running on Scaleway infrastructure.
// This cluster should be live in order to have those tests passing properly.

#[cfg(feature = "test-scw-minimal")]
#[named]
#[test]
fn scaleway_test_build_phase() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let environment = helpers::environment::working_minimal_environment(&context);

        let env_action = environment.clone();

        let (env, ret) = environment.build_environment(&env_action, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        // Check the the image exist in the registry
        let img_exist = infra_ctx
            .container_registry()
            .does_image_exists(&env.applications[0].get_build().image);
        assert!(img_exist);

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_working_environment_with_no_router() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let environment = helpers::environment::working_minimal_environment(&context);

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_for_delete = environment_for_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let result = environment_for_delete.delete_environment(&env_action_for_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_not_working_environment_with_no_router() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name,);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::non_working_environment(&context);
        environment.routers = vec![];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_for_delete = environment_for_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Error(_)));

        let result = environment_for_delete.delete_environment(&env_action_for_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok | TransactionResult::Error(_)));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_working_environment_and_pause() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let environment = helpers::environment::working_minimal_environment(&context);

        let env_action = environment.clone();
        let selector = format!("appId={}", to_short_id(&environment.applications[0].long_id));

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let ret = get_pods(&infra_ctx, Kind::Scw, environment.clone(), selector.as_str(), secrets.clone());
        assert!(ret.is_ok());
        assert!(!ret.unwrap().items.is_empty());

        let result = environment.pause_environment(&env_action, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        // Check that we have actually 0 pods running for this app
        let ret = get_pods(&infra_ctx, Kind::Scw, environment.clone(), selector.as_str(), secrets.clone());
        assert!(ret.is_ok());
        assert!(ret.unwrap().items.is_empty());

        // Check we can resume the env
        let ctx_resume = context.clone_not_same_execution_id();
        let infra_ctx_resume = scw_default_infra_config(&ctx_resume, logger.clone());
        let result = environment.deploy_environment(&env_action, &infra_ctx_resume);
        assert!(matches!(result, TransactionResult::Ok));

        let ret = get_pods(&infra_ctx, Kind::Scw, environment.clone(), selector.as_str(), secrets.clone());
        assert!(ret.is_ok());
        assert!(!ret.unwrap().items.is_empty());

        // Cleanup
        let result = environment.delete_environment(&env_action, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[ignore] // we don't want to support buildpack in middle term, and we know this one randomly fails
#[test]
fn scaleway_kapsule_build_with_buildpacks_and_deploy_a_working_environment() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name,);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let mut environment = helpers::environment::working_minimal_environment(&context);
        environment.applications = environment
            .applications
            .into_iter()
            .map(|mut app| {
                app.ports = vec![Port {
                    id: "zdf7d6aad".to_string(),
                    long_id: Default::default(),
                    port: 3000,
                    name: None,
                    is_default: true,
                    publicly_accessible: true,
                    protocol: Protocol::HTTP,
                }];
                app.commit_id = "f59237d603829636138e2f22a0549e33b5dd6e1f".to_string();
                app.branch = "simple-node-app".to_string();
                app.dockerfile_path = None;
                app
            })
            .collect::<Vec<qovery_engine::io_models::application::Application>>();

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_for_delete = environment_for_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let result = environment_for_delete.delete_environment(&env_action_for_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_working_environment_with_domain() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test",);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let environment = helpers::environment::working_minimal_environment(&context);

        let mut environment_delete = environment.clone();
        environment_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_for_delete = environment_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let result = environment_delete.delete_environment(&env_action_for_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_working_environment_with_storage() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name,);
        let _enter = span.enter();

        let secrets = FuncTestsSecrets::new();
        let logger = logger();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_deletion = context.clone_not_same_execution_id();
        let infra_ctx_for_deletion = scw_default_infra_config(&context_for_deletion, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        let storage_size: u32 = 10;
        environment.applications = environment
            .applications
            .into_iter()
            .map(|mut app| {
                let id = Uuid::new_v4();
                app.storage = vec![Storage {
                    id: to_short_id(&id),
                    long_id: id,
                    name: "photos".to_string(),
                    storage_type: StorageType::Ssd,
                    size_in_gib: storage_size,
                    mount_point: "/mnt/photos".to_string(),
                    snapshot_retention_in_days: 0,
                }];
                app
            })
            .collect::<Vec<qovery_engine::io_models::application::Application>>();

        let mut environment_delete = environment.clone();
        environment_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_delete = environment_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        match get_pvc(&infra_ctx, Kind::Scw, environment.clone(), secrets.clone()) {
            Ok(pvc) => assert_eq!(
                pvc.items.expect("No items in pvc")[0].spec.resources.requests.storage,
                format!("{storage_size}Gi")
            ),
            Err(_) => panic!(),
        };

        let result = environment_delete.delete_environment(&env_action_delete, &infra_ctx_for_deletion);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_working_environment_with_mounted_files_as_volume() {
    // TODO(benjaminch): This test could be moved out of end to end tests as it doesn't require
    // any cloud provider to be performed (can run on local Kubernetes).

    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let secrets = FuncTestsSecrets::new();
        let logger = logger();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_deletion = context.clone_not_same_execution_id();
        let infra_ctx_for_deletion = scw_default_infra_config(&context_for_deletion, logger.clone());

        let mounted_file_identifier = QoveryIdentifier::new_random();
        let mounted_file = MountedFile {
            id: mounted_file_identifier.short().to_string(),
            long_id: mounted_file_identifier.to_uuid(),
            mount_path: "/this-file-should-exist".to_string(),
            file_content_b64: base64::encode("I exist !"),
        };

        let environment =
            helpers::environment::working_environment_with_application_and_stateful_crashing_if_file_doesnt_exist(
                &context,
                &mounted_file,
            );

        let mut environment_delete = environment.clone();
        environment_delete.action = Action::Delete;

        let ea = environment.clone();
        let ea_delete = environment_delete.clone();

        let ret = environment.deploy_environment(&ea, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        // check if secret exists
        let service_id = QoveryIdentifier::new(
            environment
                .applications
                .first()
                .expect("there must be at least one application in environment")
                .long_id,
        )
        .short()
        .to_string();
        let config_maps = kubectl_get_secret(
            infra_ctx.kubernetes().kube_client().expect("kube client is not set"),
            format!("metadata.name={}-{}", &mounted_file.id, service_id).as_str(),
        )
        .expect("unable to find secret for selector");
        assert!(!config_maps.is_empty());
        for cm in config_maps {
            assert_eq!(
                base64::decode(&mounted_file.file_content_b64)
                    .expect("mounted file content cannot be b64 decoded")
                    .to_str(),
                cm.data
                    .expect("data should be set")
                    .get("content")
                    .expect("content should exist")
                    .0
                    .to_str()
            );
        }

        let ret = environment_delete.delete_environment(&ea_delete, &infra_ctx_for_deletion);
        assert!(matches!(ret, TransactionResult::Ok));

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn deploy_a_working_environment_and_pause_it() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let environment = helpers::environment::working_minimal_environment(&context);

        let ea = environment.clone();
        let selector = format!("appId={}", to_short_id(&environment.applications[0].long_id));

        let result = environment.deploy_environment(&ea, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let ret = get_pods(&infra_ctx, Kind::Scw, environment.clone(), selector.as_str(), secrets.clone());
        assert!(ret.is_ok());
        assert!(!ret.unwrap().items.is_empty());

        let result = environment.pause_environment(&ea, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        // Check that we have actually 0 pods running for this app
        let ret = get_pods(&infra_ctx, Kind::Scw, environment.clone(), selector.as_str(), secrets.clone());
        assert!(ret.is_ok());
        assert!(ret.unwrap().items.is_empty());

        // Check we can resume the env
        let ctx_resume = context.clone_not_same_execution_id();
        let infra_ctx_resume = scw_default_infra_config(&ctx_resume, logger.clone());
        let result = environment.deploy_environment(&ea, &infra_ctx_resume);
        assert!(matches!(result, TransactionResult::Ok));

        let ret = get_pods(&infra_ctx, Kind::Scw, environment.clone(), selector.as_str(), secrets);
        assert!(ret.is_ok());
        assert!(!ret.unwrap().items.is_empty());

        // Cleanup
        let result = environment.delete_environment(&ea, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));
        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_redeploy_same_app() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_bis = context.clone_not_same_execution_id();
        let infra_ctx_bis = scw_default_infra_config(&context_bis, logger.clone());
        let context_for_deletion = context.clone_not_same_execution_id();
        let infra_ctx_for_deletion = scw_default_infra_config(&context_for_deletion, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        let storage_size: u32 = 10;
        environment.applications = environment
            .applications
            .into_iter()
            .map(|mut app| {
                let id = Uuid::new_v4();
                app.storage = vec![Storage {
                    id: to_short_id(&id),
                    long_id: id,
                    name: "photos".to_string(),
                    storage_type: StorageType::Ssd,
                    size_in_gib: storage_size,
                    mount_point: "/mnt/photos".to_string(),
                    snapshot_retention_in_days: 0,
                }];
                app
            })
            .collect::<Vec<qovery_engine::io_models::application::Application>>();

        let environment_redeploy = environment.clone();
        let environment_check1 = environment.clone();
        let environment_check2 = environment.clone();
        let mut environment_delete = environment.clone();
        environment_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_redeploy = environment_redeploy.clone();
        let env_action_delete = environment_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        match get_pvc(&infra_ctx, Kind::Scw, environment.clone(), secrets.clone()) {
            Ok(pvc) => assert_eq!(
                pvc.items.expect("No items in pvc")[0].spec.resources.requests.storage,
                format!("{storage_size}Gi")
            ),
            Err(_) => panic!(),
        };

        let app_name = format!("{}-0", &environment_check1.applications[0].name);
        let (_, number) =
            is_pod_restarted_env(&infra_ctx, Kind::Scw, environment_check1, app_name.as_str(), secrets.clone());

        let result = environment_redeploy.deploy_environment(&env_action_redeploy, &infra_ctx_bis);
        assert!(matches!(result, TransactionResult::Ok));

        let (_, number2) =
            is_pod_restarted_env(&infra_ctx, Kind::Scw, environment_check2, app_name.as_str(), secrets.clone());

        // nothing changed in the app, so, it shouldn't be restarted
        assert!(number.eq(&number2));

        let result = environment_delete.delete_environment(&env_action_delete, &infra_ctx_for_deletion);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_not_working_environment_and_then_working_environment() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name,);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_not_working = context.clone_not_same_execution_id();
        let infra_ctx_for_not_working = scw_default_infra_config(&context_for_not_working, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        // env part generation
        let environment = helpers::environment::working_minimal_environment(&context);
        let mut environment_for_not_working = environment.clone();
        // this environment is broken by container exit
        environment_for_not_working.applications = environment_for_not_working
            .applications
            .into_iter()
            .map(|mut app| {
                app.git_url = "https://github.com/Qovery/engine-testing.git".to_string();
                app.branch = "1app_fail_deploy".to_string();
                app.commit_id = "5b89305b9ae8a62a1f16c5c773cddf1d12f70db1".to_string();
                app.environment_vars = BTreeMap::default();
                app
            })
            .collect::<Vec<qovery_engine::io_models::application::Application>>();

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        // environment actions
        let env_action = environment.clone();
        let env_action_not_working = environment_for_not_working.clone();
        let env_action_delete = environment_for_delete.clone();

        let result =
            environment_for_not_working.deploy_environment(&env_action_not_working, &infra_ctx_for_not_working);
        assert!(matches!(result, TransactionResult::Error(_)));

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let result = environment_for_delete.delete_environment(&env_action_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
#[ignore] // TODO(benjaminch): Make it work (it doesn't work on AWS neither)
fn scaleway_kapsule_deploy_ok_fail_fail_ok_environment() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();

        // working env

        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let environment = helpers::environment::working_minimal_environment(&context);

        // not working 1
        let context_for_not_working_1 = context.clone_not_same_execution_id();
        let infra_ctx_for_not_working_1 = scw_default_infra_config(&context_for_not_working_1, logger.clone());
        let mut not_working_env_1 = environment.clone();
        not_working_env_1.applications = not_working_env_1
            .applications
            .into_iter()
            .map(|mut app| {
                app.git_url = "https://gitlab.com/maathor/my-exit-container".to_string();
                app.branch = "master".to_string();
                app.commit_id = "55bc95a23fbf91a7699c28c5f61722d4f48201c9".to_string();
                app.environment_vars = BTreeMap::default();
                app
            })
            .collect::<Vec<qovery_engine::io_models::application::Application>>();

        // not working 2
        let context_for_not_working_2 = context.clone_not_same_execution_id();
        let infra_ctx_for_not_working_2 = scw_default_infra_config(&context_for_not_working_2, logger.clone());
        let not_working_env_2 = not_working_env_1.clone();

        // work for delete
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let mut delete_env = environment.clone();
        delete_env.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_not_working_1 = not_working_env_1.clone();
        let env_action_not_working_2 = not_working_env_2.clone();
        let env_action_delete = delete_env.clone();

        // OK
        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        // FAIL and rollback
        let result = not_working_env_1.deploy_environment(&env_action_not_working_1, &infra_ctx_for_not_working_1);
        assert!(matches!(result, TransactionResult::Error(_)));

        // FAIL and Rollback again
        let result = not_working_env_2.deploy_environment(&env_action_not_working_2, &infra_ctx_for_not_working_2);
        assert!(matches!(result, TransactionResult::Error(_)));

        // Should be working
        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        let result = delete_env.delete_environment(&env_action_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[named]
#[test]
fn scaleway_kapsule_deploy_a_non_working_environment_with_no_failover() {
    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name,);
        let _enter = span.enter();

        let logger = logger();

        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let environment = helpers::environment::non_working_environment(&context);

        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let mut delete_env = environment.clone();
        delete_env.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_delete = delete_env.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Error(_)));

        let result = delete_env.delete_environment(&env_action_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[ignore] // TODO: fix main ingress to let it handle sticky session
#[named]
#[test]
fn scaleway_kapsule_deploy_a_working_environment_with_sticky_session() {
    use qovery_engine::models::router::RouterAdvancedSettings;

    let test_name = function_name!();
    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let region = ScwZone::from_str(
            secrets
                .SCALEWAY_TEST_CLUSTER_REGION
                .as_ref()
                .expect("SCALEWAY_TEST_CLUSTER_REGION is not set")
                .to_string()
                .as_str(),
        )
        .expect("Unknown SCW region");
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());
        let environment = helpers::environment::environment_only_http_server_router_with_sticky_session(
            &context,
            secrets
                .DEFAULT_TEST_DOMAIN
                .as_ref()
                .expect("DEFAULT_TEST_DOMAIN is not set in secrets")
                .as_str(),
        );

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let env_action = environment.clone();
        let env_action_for_delete = environment_for_delete.clone();

        let result = environment.deploy_environment(&env_action, &infra_ctx);
        assert!(matches!(result, TransactionResult::Ok));

        // checking cookie is properly set on the app
        let kubeconfig = infra_ctx.kubernetes().get_kubeconfig_file_path();
        assert!(kubeconfig.is_ok());
        let router = environment
            .routers
            .first()
            .unwrap()
            .to_router_domain(
                infra_ctx.context(),
                RouterAdvancedSettings::new(true, None, None, None),
                infra_ctx.cloud_provider(),
            )
            .unwrap();
        let environment_domain = environment
            .to_environment_domain(
                infra_ctx.context(),
                infra_ctx.cloud_provider(),
                infra_ctx.container_registry(),
                infra_ctx.kubernetes(),
            )
            .unwrap();

        // let some time for ingress to get its IP or hostname
        // Sticky session is checked on ingress IP or hostname so we are not subjects to long DNS propagation making test less flacky.
        let ingress = retry::retry(Fibonacci::from_millis(15000).take(8), || {
            match qovery_engine::cmd::kubectl::kubectl_exec_get_external_ingress(
                kubeconfig.as_ref().unwrap().as_str(),
                environment_domain.namespace(),
                router.sanitized_name().as_str(),
                infra_ctx.cloud_provider().credentials_environment_variables(),
            ) {
                Ok(res) => match res {
                    Some(res) => retry::OperationResult::Ok(res),
                    None => retry::OperationResult::Retry("ingress not found"),
                },
                Err(_) => retry::OperationResult::Retry("cannot get ingress"),
            }
        })
        .expect("cannot get ingress");
        let ingress_host = ingress
            .ip
            .as_ref()
            .unwrap_or_else(|| ingress.hostname.as_ref().expect("ingress has no IP nor hostname"));

        for router in environment.routers.iter() {
            for route in router.routes.iter() {
                assert!(session_is_sticky(
                    Url::parse(format!("http://{}{}", ingress_host, route.path).as_str()).expect("cannot parse URL"),
                    router.default_domain.clone(),
                    85400,
                ));
            }
        }

        let result = environment_for_delete.delete_environment(&env_action_for_delete, &infra_ctx_for_delete);
        assert!(matches!(result, TransactionResult::Ok));

        if let Err(e) = clean_environments(&context, vec![environment], secrets, region) {
            warn!("cannot clean environments, error: {:?}", e);
        }

        test_name.to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[named]
#[test]
fn deploy_container_with_no_router_on_scw() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = function_name!());
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        environment.routers = vec![];
        environment.applications = vec![];
        environment.containers = vec![Container {
            long_id: Uuid::new_v4(),
            name: "👾👾👾 my little container 澳大利亚和智利提及年度采购计划 👾👾👾".to_string(),
            action: Action::Create,
            registry: Registry::PublicEcr {
                long_id: Uuid::new_v4(),
                url: Url::parse("https://public.ecr.aws").unwrap(),
            },
            image: "r3m4q3r9/pub-mirror-debian".to_string(),
            tag: "11.6-ci".to_string(),
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "apt-get update; apt-get install -y netcat; echo listening on port $PORT; env ; while true; do nc -l 8080; done".to_string(),
            ],
            entrypoint: None,
            cpu_request_in_mili: 250,
            cpu_limit_in_mili: 250,
            ram_request_in_mib: 250,
            ram_limit_in_mib: 250,
            min_instances: 1,
            max_instances: 1,
            ports: vec![
                Port {
                    long_id: Uuid::new_v4(),
                    id: Uuid::new_v4().to_string(),
                    port: 8080,
                    name: Some("http".to_string()),
                    is_default: true,
                    publicly_accessible: true,
                    protocol: Protocol::HTTP,
                },
                Port {
                    long_id: Uuid::new_v4(),
                    id: Uuid::new_v4().to_string(),
                    port: 8081,
                    name: Some("grpc".to_string()),
                    is_default: false,
                    publicly_accessible: false,
                    protocol: Protocol::HTTP,
                },
            ],
            storages: vec![],
            environment_vars: btreemap! { "MY_VAR".to_string() => base64::encode("my_value") },
            mounted_files: vec![],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[named]
#[test]
fn deploy_container_on_scw_with_mounted_files_as_volume() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = function_name!());
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        let mounted_file_identifier = QoveryIdentifier::new_random();
        let mounted_file = MountedFile {
            id: mounted_file_identifier.short().to_string(),
            long_id: mounted_file_identifier.to_uuid(),
            mount_path: "/this-file-should-exist".to_string(),
            file_content_b64: base64::encode("I exist !"),
        };

        environment.routers = vec![];
        environment.applications = vec![];
        environment.containers = vec![Container {
            long_id: Uuid::new_v4(),
            name: "👾👾👾 my little container 澳大利亚和智利提及年度采购计划 👾👾👾".to_string(),
            action: Action::Create,
            registry: Registry::PublicEcr {
                long_id: Uuid::new_v4(),
                url: Url::parse("https://public.ecr.aws").unwrap(),
            },
            image: "r3m4q3r9/pub-mirror-debian".to_string(),
            tag: "11.6-ci".to_string(),
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                format!("apt-get update; apt-get install -y netcat; echo listening on port $PORT; env ; cat {} ; while true; do nc -l 8080; done", &mounted_file.mount_path),
            ],
            entrypoint: None,
            cpu_request_in_mili: 250,
            cpu_limit_in_mili: 250,
            ram_request_in_mib: 250,
            ram_limit_in_mib: 250,
            min_instances: 1,
            max_instances: 1,
            ports: vec![
                Port {
                    long_id: Uuid::new_v4(),
                    id: Uuid::new_v4().to_string(),
                    port: 8080,
                    name: Some("http".to_string()),
                    is_default: true,
                    publicly_accessible: true,
                    protocol: Protocol::HTTP,
                },
                Port {
                    long_id: Uuid::new_v4(),
                    id: Uuid::new_v4().to_string(),
                    port: 8081,
                    name: Some("grpc".to_string()),
                    is_default: false,
                    publicly_accessible: false,
                    protocol: Protocol::HTTP,
                },
            ],
            storages: vec![],
            environment_vars: btreemap! { "MY_VAR".to_string() => base64::encode("my_value") },
            mounted_files: vec![mounted_file.clone()],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        // check if secret exists
        let service_id = QoveryIdentifier::new(
            environment
                .containers
                .first()
                .expect("there must be at least one container in environment")
                .long_id,
        )
        .short()
        .to_string();
        let config_maps = kubectl_get_secret(
            infra_ctx.kubernetes().kube_client().expect("kube client is not set"),
            format!("metadata.name={}-{}", &mounted_file.id, service_id).as_str(),
        )
        .expect("unable to find secret for selector");
        assert!(!config_maps.is_empty());
        for cm in config_maps {
            assert_eq!(
                base64::decode(&mounted_file.file_content_b64)
                    .expect("mounted file content cannot be b64 decoded")
                    .to_str(),
                cm.data
                    .expect("data should be set")
                    .get("content")
                    .expect("content should exist")
                    .0
                    .to_str()
            );
        }

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[named]
#[test]
fn deploy_container_with_router_on_scw() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = function_name!());
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        environment.applications = vec![];
        environment.containers = vec![Container {
            long_id: Uuid::new_v4(),
            name: "👾👾👾 my little container 澳大利亚和智利提及年度采购计划 👾👾👾".to_string(),
            action: Action::Create,
            registry: Registry::DockerHub {
                url: Url::parse("https://public.ecr.aws").unwrap(),
                long_id: Uuid::new_v4(),
                credentials: None,
            },
            image: "r3m4q3r9/pub-mirror-httpd".to_string(),
            tag: "2.4.56-alpine3.17".to_string(),
            command_args: vec![],
            entrypoint: None,
            cpu_request_in_mili: 250,
            cpu_limit_in_mili: 250,
            ram_request_in_mib: 250,
            ram_limit_in_mib: 250,
            min_instances: 1,
            max_instances: 1,
            ports: vec![
                Port {
                    long_id: Uuid::new_v4(),
                    id: Uuid::new_v4().to_string(),
                    port: 80,
                    name: Some("http".to_string()),
                    is_default: true,
                    publicly_accessible: true,
                    protocol: Protocol::HTTP,
                },
                Port {
                    long_id: Uuid::new_v4(),
                    id: Uuid::new_v4().to_string(),
                    port: 8081,
                    name: Some("grpc".to_string()),
                    is_default: false,
                    publicly_accessible: false,
                    protocol: Protocol::HTTP,
                },
            ],
            storages: vec![],
            environment_vars: btreemap! { "MY_VAR".to_string() => base64::encode("my_value") },
            mounted_files: vec![],
            advanced_settings: Default::default(),
        }];

        environment.routers = vec![Router {
            long_id: Uuid::new_v4(),
            name: "default-router".to_string(),
            action: Action::Create,
            default_domain: "main".to_string(),
            public_port: 443,
            custom_domains: vec![],
            routes: vec![Route {
                path: "/".to_string(),
                service_long_id: environment.containers[0].long_id,
            }],
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[test]
fn deploy_job_on_scw_kapsule() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = "deploy_job_on_scw_kapsule");
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID is not set"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID is not set"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        let json_output = "{\"foo\": {\"value\": \"bar\", \"sensitive\": true}, \"foo_2\": {\"value\": \"bar_2\"}}";
        //environment.long_id = Uuid::default();
        //environment.project_long_id = Uuid::default();
        environment.applications = vec![];
        environment.jobs = vec![Job {
            long_id: Uuid::new_v4(), //Uuid::default(),
            name: "job test #####".to_string(),
            action: Action::Create,
            schedule: JobSchedule::OnStart {}, //JobSchedule::Cron("* * * * *".to_string()),
            source: JobSource::Image {
                registry: Registry::PublicEcr {
                    long_id: Uuid::new_v4(),
                    url: Url::parse("https://public.ecr.aws").unwrap(),
                },
                image: "r3m4q3r9/pub-mirror-debian".to_string(),
                tag: "11.6-ci".to_string(),
            },
            max_nb_restart: 2,
            max_duration_in_sec: 300,
            default_port: Some(8080),
            //command_args: vec![],
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                format!("echo starting; sleep 10; echo '{json_output}' > /qovery-output/qovery-output.json"),
            ],
            entrypoint: None,
            force_trigger: false,
            cpu_request_in_milli: 100,
            cpu_limit_in_milli: 100,
            ram_request_in_mib: 100,
            ram_limit_in_mib: 100,
            environment_vars: Default::default(),
            mounted_files: vec![],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[test]
fn deploy_cronjob_on_scw_kapsule() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = "deploy_cronjob_on_scw_kapsule");
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID is not set"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID is not set"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        environment.applications = vec![];
        environment.jobs = vec![Job {
            long_id: Uuid::new_v4(),
            name: "job test #####||||*_-(".to_string(),
            action: Action::Create,
            schedule: JobSchedule::Cron {
                schedule: "* * * * *".to_string(),
            },
            source: JobSource::Image {
                registry: Registry::PublicEcr {
                    long_id: Uuid::new_v4(),
                    url: Url::parse("https://public.ecr.aws").unwrap(),
                },
                image: "r3m4q3r9/pub-mirror-debian".to_string(),
                tag: "11.6-ci".to_string(),
            },
            max_nb_restart: 1,
            max_duration_in_sec: 30,
            default_port: Some(8080),
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "echo starting; sleep 10; echo started".to_string(),
            ],
            entrypoint: None,
            force_trigger: false,
            cpu_request_in_milli: 100,
            cpu_limit_in_milli: 100,
            ram_request_in_mib: 100,
            ram_limit_in_mib: 100,
            environment_vars: Default::default(),
            mounted_files: vec![],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-self-hosted")]
#[test]
fn deploy_cronjob_force_trigger_on_scw_kapsule() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = "deploy_cronjob_on_scw_kapsule");
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID is not set"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID is not set"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        environment.applications = vec![];
        environment.jobs = vec![Job {
            long_id: Uuid::new_v4(),
            name: "job test #####||||*_-(".to_string(),
            action: Action::Create,
            schedule: JobSchedule::Cron {
                schedule: "*/10 * * * *".to_string(),
            },
            source: JobSource::Image {
                registry: Registry::PublicEcr {
                    long_id: Uuid::new_v4(),
                    url: Url::parse("https://public.ecr.aws").unwrap(),
                },
                image: "r3m4q3r9/pub-mirror-debian".to_string(),
                tag: "11.6-ci".to_string(),
            },
            max_nb_restart: 1,
            max_duration_in_sec: 30,
            default_port: Some(8080),
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                "echo starting; sleep 10; echo started".to_string(),
            ],
            entrypoint: None,
            force_trigger: true,
            cpu_request_in_milli: 100,
            cpu_limit_in_milli: 100,
            ram_request_in_mib: 100,
            ram_limit_in_mib: 100,
            environment_vars: Default::default(),
            mounted_files: vec![],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[test]
fn build_and_deploy_job_on_scw_kapsule() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = "build_and_deploy_job_on_scw_kapsule");
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID is not set"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID is not set"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mut environment = helpers::environment::working_minimal_environment(&context);

        let json_output = "{\"foo\": {\"value\": \"bar\", \"sensitive\": true}, \"foo_2\": {\"value\": \"bar_2\"}}";
        environment.applications = vec![];
        environment.jobs = vec![Job {
            long_id: Uuid::new_v4(),
            name: "job test #####".to_string(),
            action: Action::Create,
            schedule: JobSchedule::OnStart {},
            source: JobSource::Docker {
                git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
                commit_id: "fc575a2f3be0b9100492c8a463bf18134a8698a5".to_string(),
                dockerfile_path: Some("Dockerfile".to_string()),
                root_path: String::from("/"),
                git_credentials: None,
                branch: "main".to_string(),
            },
            max_nb_restart: 2,
            max_duration_in_sec: 300,
            default_port: Some(8080),
            //command_args: vec![],
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                format!("echo starting; sleep 10; echo '{json_output}' > /qovery-output/qovery-output.json"),
            ],
            entrypoint: None,
            force_trigger: false,
            cpu_request_in_milli: 100,
            cpu_limit_in_milli: 100,
            ram_request_in_mib: 100,
            ram_limit_in_mib: 100,
            environment_vars: Default::default(),
            mounted_files: vec![],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}

#[cfg(feature = "test-scw-minimal")]
#[test]
fn build_and_deploy_job_on_scw_kapsule_with_mounted_files() {
    engine_run_test(|| {
        init();
        let span = span!(Level::INFO, "test", name = "build_and_deploy_job_on_scw_kapsule");
        let _enter = span.enter();

        let logger = logger();
        let secrets = FuncTestsSecrets::new();
        let context = context_for_resource(
            secrets
                .SCALEWAY_TEST_ORGANIZATION_LONG_ID
                .expect("SCALEWAY_TEST_ORGANIZATION_LONG_ID is not set"),
            secrets
                .SCALEWAY_TEST_CLUSTER_LONG_ID
                .expect("SCALEWAY_TEST_CLUSTER_LONG_ID is not set"),
        );
        let infra_ctx = scw_default_infra_config(&context, logger.clone());
        let context_for_delete = context.clone_not_same_execution_id();
        let infra_ctx_for_delete = scw_default_infra_config(&context_for_delete, logger.clone());

        let mounted_file_identifier = QoveryIdentifier::new_random();
        let mounted_file = MountedFile {
            id: mounted_file_identifier.short().to_string(),
            long_id: mounted_file_identifier.to_uuid(),
            mount_path: "/this-file-should-exist.json".to_string(),
            file_content_b64: base64::encode(
                "{\"foo\": {\"value\": \"bar\", \"sensitive\": true}, \"foo_2\": {\"value\": \"bar_2\"}}",
            ),
        };

        let mut environment = helpers::environment::working_minimal_environment(&context);

        environment.applications = vec![];
        environment.jobs = vec![Job {
            long_id: Uuid::new_v4(),
            name: "job test #####".to_string(),
            action: Action::Create,
            schedule: JobSchedule::OnStart {},
            source: JobSource::Docker {
                git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
                commit_id: "fc575a2f3be0b9100492c8a463bf18134a8698a5".to_string(),
                dockerfile_path: Some("Dockerfile".to_string()),
                root_path: String::from("/"),
                git_credentials: None,
                branch: "main".to_string(),
            },
            max_nb_restart: 2,
            max_duration_in_sec: 300,
            default_port: Some(8080),
            //command_args: vec![],
            command_args: vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                format!(
                    "echo starting; sleep 10; cat {} > /qovery-output/qovery-output.json",
                    &mounted_file.mount_path,
                ),
            ],
            entrypoint: None,
            force_trigger: false,
            cpu_request_in_milli: 100,
            cpu_limit_in_milli: 100,
            ram_request_in_mib: 100,
            ram_limit_in_mib: 100,
            environment_vars: Default::default(),
            mounted_files: vec![mounted_file.clone()],
            advanced_settings: Default::default(),
        }];

        let mut environment_for_delete = environment.clone();
        environment_for_delete.action = Action::Delete;

        let ret = environment.deploy_environment(&environment, &infra_ctx);
        assert!(matches!(ret, TransactionResult::Ok));

        // check if secret exists
        let service_id = QoveryIdentifier::new(
            environment
                .jobs
                .first()
                .expect("there must be at least one job in environment")
                .long_id,
        )
        .short()
        .to_string();
        let config_maps = kubectl_get_secret(
            infra_ctx.kubernetes().kube_client().expect("kube client is not set"),
            format!("metadata.name={}-{}", &mounted_file.id, service_id).as_str(),
        )
        .expect("unable to find secret for selector");
        assert!(!config_maps.is_empty());
        for cm in config_maps {
            assert_eq!(
                base64::decode(&mounted_file.file_content_b64)
                    .expect("mounted file content cannot be b64 decoded")
                    .to_str(),
                cm.data
                    .expect("data should be set")
                    .get("content")
                    .expect("content should exist")
                    .0
                    .to_str()
            );
        }

        let ret = environment_for_delete.delete_environment(&environment_for_delete, &infra_ctx_for_delete);
        assert!(matches!(ret, TransactionResult::Ok));

        "".to_string()
    })
}
