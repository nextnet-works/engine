use crate::helpers::common::Infrastructure;
use crate::helpers::database::StorageSize::Resize;
use crate::helpers::utilities::{engine_run_test, init};
use crate::kube::{kube_test_env, TestEnvOption};
use function_name::named;
use k8s_openapi::api::core::v1::PersistentVolumeClaim;
use qovery_engine::cloud_provider::models::{EnvironmentVariable, Storage};
use qovery_engine::cloud_provider::service::ServiceType;
use qovery_engine::cloud_provider::utilities::update_pvcs;
use qovery_engine::cloud_provider::DeploymentTarget;
use qovery_engine::io_models::context::CloneForTest;
use qovery_engine::io_models::Action;
use qovery_engine::kubers_utils::kube_get_resources_by_selector;
use qovery_engine::models::application::{get_application_with_invalid_storage_size, Application};
use qovery_engine::models::aws::{AwsAppExtraSettings, AwsStorageType};
use qovery_engine::models::types::AWS;
use qovery_engine::runtime::block_on;
use qovery_engine::transaction::TransactionResult;
use std::collections::BTreeSet;
use tracing::{span, Level};

#[cfg(feature = "test-local-kube")]
#[test]
#[named]
fn should_increase_app_storage_size() {
    let test_name = function_name!();

    engine_run_test(|| {
        init();

        let span = span!(Level::INFO, "test", name = test_name);
        let _enter = span.enter();

        let (infra_ctx, environment) = kube_test_env(TestEnvOption::WithApp);
        let ea = environment.clone();

        assert!(matches!(environment.deploy_environment(&ea, &infra_ctx), TransactionResult::Ok));

        let mut resized_env = environment.clone();
        resized_env.applications[0].storage[0].size_in_gib = Resize.size();
        let resized_app = &resized_env.applications[0];

        let resized_context = infra_ctx.context().clone_not_same_execution_id();
        let test_env = resized_env
            .to_environment_domain(&resized_context, infra_ctx.cloud_provider(), infra_ctx.container_registry())
            .unwrap();
        let deployment_target = DeploymentTarget::new(&infra_ctx, &test_env, &|| false).unwrap();
        let test_app = &test_env.applications[0];

        let storages = resized_app
            .storage
            .iter()
            .map(|storage| storage.to_aws_storage())
            .collect::<Vec<Storage<AwsStorageType>>>();

        let envs = resized_app
            .environment_vars
            .iter()
            .map(|(k, v)| EnvironmentVariable {
                key: k.to_string(),
                value: v.to_string(),
            })
            .collect::<Vec<EnvironmentVariable>>();
        let app: Application<AWS> = Application::new(
            &resized_context,
            resized_app.long_id,
            *test_app.action(),
            resized_app.name.as_str(),
            resized_app.ports.clone(),
            resized_app.total_cpus.to_string(),
            resized_app.cpu_burst.to_string(),
            resized_app.total_ram_in_mib,
            resized_app.min_instances,
            resized_app.max_instances,
            resized_app.to_build(
                infra_ctx.container_registry().registry_info(),
                infra_ctx.context().qovery_api.clone(),
            ),
            resized_app.command_args.clone(),
            resized_app.entrypoint.clone(),
            storages,
            envs,
            BTreeSet::default(),
            resized_app.advanced_settings.clone(),
            AwsAppExtraSettings {},
            |transmitter| infra_ctx.context().get_event_details(transmitter),
        )
        .expect("Unable to create application");

        let invalid_statefulset = match get_application_with_invalid_storage_size(
            &app,
            &deployment_target.kube,
            deployment_target.environment.namespace(),
            deployment_target.environment.event_details(),
        ) {
            Ok(result) => match result {
                Some(invalid_storage) => {
                    assert_eq!(invalid_storage.service_type, ServiceType::Application);
                    assert_eq!(invalid_storage.service_id, test_app.long_id().clone());
                    assert_eq!(invalid_storage.invalid_pvcs.len(), 1);
                    assert_eq!(invalid_storage.invalid_pvcs[0].required_disk_size_in_gib, Resize.size());
                    assert!(invalid_storage.invalid_pvcs[0]
                        .pvc_name
                        .starts_with(&resized_env.applications[0].storage[0].id));
                    invalid_storage
                }
                None => panic!("No invalid storage returned"),
            },
            Err(e) => panic!("No invalid storage returned: {e}"),
        };

        let ret = update_pvcs(
            test_app.as_service(),
            &invalid_statefulset,
            test_env.namespace(),
            test_env.event_details(),
            &deployment_target.kube,
        );
        assert!(ret.is_ok());

        //assert app can be redeployed
        let rea = resized_env.clone();
        assert!(matches!(
            resized_env.deploy_environment(&rea, &infra_ctx),
            TransactionResult::Ok
        ));

        // assert edited storage have good size
        let pvcs = match block_on(kube_get_resources_by_selector::<PersistentVolumeClaim>(
            &deployment_target.kube,
            deployment_target.environment.namespace(),
            &invalid_statefulset.statefulset_selector,
        )) {
            Ok(result) => result.items,
            Err(_) => panic!("Unable to get pvcs"),
        };

        let pvc = pvcs
            .iter()
            .find(|pvc| match &pvc.metadata.name {
                Some(name) => *name.to_string() == invalid_statefulset.invalid_pvcs[0].pvc_name,
                None => false,
            })
            .expect("Unable to get pvc");

        if let Some(spec) = &pvc.spec {
            if let Some(resources) = &spec.resources {
                if let Some(req) = &resources.requests {
                    assert!(
                        req["storage"].0
                            == format!("{}Gi", invalid_statefulset.invalid_pvcs[0].required_disk_size_in_gib)
                    )
                }
            }
        }

        // clean up
        let mut env_to_delete = environment;
        env_to_delete.action = Action::Delete;
        let ead = env_to_delete.clone();
        assert!(matches!(
            env_to_delete.delete_environment(&ead, &infra_ctx),
            TransactionResult::Ok
        ));

        test_name.to_string()
    });
}