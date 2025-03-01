use crate::helpers::utilities::{generate_id, generate_password, get_svc_name};
use chrono::Utc;
use qovery_engine::cloud_provider::utilities::sanitize_name;
use qovery_engine::cloud_provider::Kind;
use qovery_engine::io_models::application::{Application, ApplicationAdvancedSettings, Port, Protocol, StorageType};
use qovery_engine::io_models::context::Context;
use qovery_engine::io_models::database::DatabaseMode::CONTAINER;
use qovery_engine::io_models::database::{Database, DatabaseKind};
use qovery_engine::io_models::environment::EnvironmentRequest;
use qovery_engine::io_models::router::{Route, Router};
use qovery_engine::io_models::{Action, MountedFile, QoveryIdentifier};
use std::collections::BTreeMap;
use tracing::error;
use url::Url;
use uuid::Uuid;

pub fn working_environment(
    context: &Context,
    test_domain: &str,
    with_router: bool,
    with_sticky: bool,
) -> EnvironmentRequest {
    let application_id = QoveryIdentifier::new_random();
    let application_name = application_id.short().to_string();
    let router_name = "main".to_string();
    let application_domain = format!("{}.{}.{}", application_name, context.cluster_short_id(), test_domain);
    let settings = ApplicationAdvancedSettings {
        network_ingress_sticky_session_enable: with_sticky,
        ..Default::default()
    };

    let mut req = EnvironmentRequest {
        execution_id: context.execution_id().to_string(),
        long_id: application_id.to_uuid(),
        name: "env".to_string(),
        project_long_id: Uuid::new_v4(),
        organization_long_id: Uuid::new_v4(),
        action: Action::Create,
        max_parallel_build: 1,
        applications: vec![Application {
            long_id: application_id.to_uuid(),
            name: application_name,
            git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
            commit_id: "4bc6a902e83129a118185660b3c9e13dfd0ffc27".to_string(),
            dockerfile_path: Some("Dockerfile".to_string()),
            command_args: vec![],
            entrypoint: None,
            buildpack_language: None,
            root_path: String::from("/"),
            action: Action::Create,
            git_credentials: None,
            storage: vec![],
            environment_vars: BTreeMap::default(),
            mounted_files: vec![],
            branch: "basic-app-deploy".to_string(),
            ports: vec![Port {
                id: "zdf7d6aad".to_string(),
                long_id: Default::default(),
                port: 80,
                is_default: true,
                name: None,
                publicly_accessible: true,
                protocol: Protocol::HTTP,
            }],
            total_cpus: "100m".to_string(),
            total_ram_in_mib: 256,
            min_instances: 1,
            max_instances: 1,
            cpu_burst: "100m".to_string(),
            advanced_settings: settings,
        }],
        containers: vec![],
        jobs: vec![],
        routers: vec![],
        databases: vec![],
    };

    if with_router {
        req.routers = vec![Router {
            long_id: Uuid::new_v4(),
            name: router_name,
            action: Action::Create,
            default_domain: application_domain,
            public_port: 443,
            custom_domains: vec![],
            routes: vec![Route {
                path: "/".to_string(),
                service_long_id: application_id.to_uuid(),
            }],
        }]
    }

    req
}

pub fn working_minimal_environment(context: &Context) -> EnvironmentRequest {
    working_environment(context, "", false, false)
}

pub fn working_minimal_environment_with_router(context: &Context, test_domain: &str) -> EnvironmentRequest {
    working_environment(context, test_domain, true, false)
}

pub fn working_environment_with_application_and_stateful_crashing_if_file_doesnt_exist(
    context: &Context,
    mounted_file: &MountedFile,
) -> EnvironmentRequest {
    let mut environment = working_environment(context, "", false, false);

    let mut application = environment
        .applications
        .first()
        .expect("there is no application in env")
        .clone();

    // removing useless objects for this test
    environment.containers = vec![];
    environment.databases = vec![];
    environment.jobs = vec![];
    environment.routers = vec![];

    let mount_file_env_var_key = "APP_CONFIG";
    let mount_file_env_var_value = mounted_file.mount_path.to_string();

    // Use an app crashing in case file doesn't exists
    // todo: move this to pure shell to speed up CI
    application.git_url = "https://github.com/Qovery/engine-testing.git".to_string();
    application.branch = "app-crashing-if-file-doesnt-exist".to_string();
    application.commit_id = "44b889f36c81cce7dee678993bb7986c86899e5d".to_string();
    application.ports = vec![];
    application.mounted_files = vec![mounted_file.clone()];
    application.environment_vars = BTreeMap::from([
        (
            "APP_FILE_PATH_TO_BE_CHECKED".to_string(),
            base64::encode(&mount_file_env_var_value),
        ), // <- https://github.com/Qovery/engine-testing/blob/app-crashing-if-file-doesnt-exist/src/main.rs#L19
        (mount_file_env_var_key.to_string(), base64::encode(&mount_file_env_var_value)), // <- mounted file PATH
    ]);

    // create a statefulset
    let mut statefulset = application.clone();
    let statefulset_id = QoveryIdentifier::new_random();
    statefulset.name = statefulset_id.short().to_string();
    statefulset.long_id = statefulset_id.to_uuid();
    let storage_id = QoveryIdentifier::new_random();
    statefulset.storage = vec![qovery_engine::io_models::application::Storage {
        id: storage_id.short().to_string(),
        long_id: storage_id.to_uuid(),
        name: storage_id.short().to_string(),
        storage_type: StorageType::Ssd,
        size_in_gib: 10,
        mount_point: format!("/tmp/{}", storage_id.short()),
        snapshot_retention_in_days: 1,
    }];

    // attaching application & statefulset to env
    environment.applications = vec![application, statefulset];

    environment
}

pub fn environment_2_app_2_routers_1_psql(
    context: &Context,
    test_domain: &str,
    database_instance_type: &str,
    database_disk_type: &str,
    provider_kind: Kind,
) -> EnvironmentRequest {
    let fqdn = get_svc_name(DatabaseKind::Postgresql, provider_kind).to_string();

    let database_port = 5432;
    let database_username = "superuser".to_string();
    let database_password = generate_password(CONTAINER);
    let database_name = "pg".to_string();

    let suffix = QoveryIdentifier::new_random().short().to_string();
    let application_id1 = Uuid::new_v4();
    let application_id2 = Uuid::new_v4();

    EnvironmentRequest {
        execution_id: context.execution_id().to_string(),
        long_id: Uuid::new_v4(),
        name: "env".to_string(),
        project_long_id: Uuid::new_v4(),
        organization_long_id: Uuid::new_v4(),
        action: Action::Create,
        databases: vec![Database {
            kind: DatabaseKind::Postgresql,
            action: Action::Create,
            long_id: Uuid::new_v4(),
            name: database_name.clone(),
            created_at: Utc::now(),
            version: "11.8.0".to_string(),
            fqdn_id: fqdn.clone(),
            fqdn: fqdn.clone(),
            port: database_port,
            username: database_username.clone(),
            password: database_password.clone(),
            total_cpus: "100m".to_string(),
            total_ram_in_mib: 512,
            disk_size_in_gib: 10,
            database_instance_type: database_instance_type.to_string(),
            database_disk_type: database_disk_type.to_string(),
            encrypt_disk: true,
            activate_high_availability: false,
            activate_backups: false,
            publicly_accessible: false,
            mode: CONTAINER,
        }],
        applications: vec![
            Application {
                long_id: application_id1,
                name: sanitize_name("pg", &format!("{}-{}", "pg-app1", &suffix)),
                git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
                branch: "postgres-app".to_string(),
                commit_id: "71990e977a60c87034530614607494a96dee2254".to_string(),
                dockerfile_path: Some("Dockerfile-11".to_string()),
                command_args: vec![],
                entrypoint: None,
                buildpack_language: None,
                root_path: String::from("/"),
                action: Action::Create,
                git_credentials: None,
                storage: vec![],
                environment_vars: btreemap! {
                     "PG_DBNAME".to_string() => base64::encode(database_name.clone()),
                     "PG_HOST".to_string() => base64::encode(fqdn.clone()),
                     "PG_PORT".to_string() => base64::encode(database_port.to_string()),
                     "PG_USERNAME".to_string() => base64::encode(database_username.clone()),
                     "PG_PASSWORD".to_string() => base64::encode(database_password.clone()),
                },
                mounted_files: vec![],
                ports: vec![Port {
                    id: "zdf7d6aad".to_string(),
                    long_id: Default::default(),
                    port: 1234,
                    is_default: true,
                    name: None,
                    publicly_accessible: true,
                    protocol: Protocol::TCP,
                }],
                total_cpus: "100m".to_string(),
                total_ram_in_mib: 256,
                min_instances: 1,
                max_instances: 1,
                cpu_burst: "100m".to_string(),
                advanced_settings: Default::default(),
            },
            Application {
                long_id: application_id2,
                name: sanitize_name("pg", &format!("{}-{}", "pg-app2", &suffix)),
                git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
                branch: "postgres-app".to_string(),
                commit_id: "71990e977a60c87034530614607494a96dee2254".to_string(),
                dockerfile_path: Some("Dockerfile-11".to_string()),
                command_args: vec![],
                entrypoint: None,
                buildpack_language: None,
                root_path: String::from("/"),
                action: Action::Create,
                git_credentials: None,
                storage: vec![],
                environment_vars: btreemap! {
                     "PG_DBNAME".to_string() => base64::encode(database_name),
                     "PG_HOST".to_string() => base64::encode(fqdn),
                     "PG_PORT".to_string() => base64::encode(database_port.to_string()),
                     "PG_USERNAME".to_string() => base64::encode(database_username),
                     "PG_PASSWORD".to_string() => base64::encode(database_password),
                },
                mounted_files: vec![],
                ports: vec![Port {
                    id: "zdf7d6aad".to_string(),
                    long_id: Default::default(),
                    port: 1234,
                    is_default: true,
                    name: None,
                    publicly_accessible: true,
                    protocol: Protocol::TCP,
                }],
                total_cpus: "100m".to_string(),
                total_ram_in_mib: 256,
                min_instances: 1,
                max_instances: 1,
                cpu_burst: "100m".to_string(),
                advanced_settings: Default::default(),
            },
        ],
        containers: vec![],
        jobs: vec![],
        routers: vec![
            Router {
                long_id: Uuid::new_v4(),
                name: "main".to_string(),
                action: Action::Create,
                default_domain: format!("{}.{}.{}", generate_id(), context.cluster_short_id(), test_domain),
                public_port: 443,
                custom_domains: vec![],
                routes: vec![Route {
                    path: "/".to_string(),
                    service_long_id: application_id1,
                }],
            },
            Router {
                long_id: Uuid::new_v4(),
                name: "second-router".to_string(),
                action: Action::Create,
                default_domain: format!("{}.{}.{}", generate_id(), context.cluster_short_id(), test_domain),
                public_port: 443,
                custom_domains: vec![],
                routes: vec![Route {
                    path: "/coco".to_string(),
                    service_long_id: application_id2,
                }],
            },
        ],
        max_parallel_build: 1,
    }
}

pub fn non_working_environment(context: &Context) -> EnvironmentRequest {
    let mut environment = working_environment(context, "", false, false);
    environment.applications = environment
        .applications
        .into_iter()
        .map(|mut app| {
            app.git_url = "https://github.com/Qovery/engine-testing.git".to_string();
            app.branch = "bugged-image".to_string();
            app.commit_id = "8feceb20eddb57872b086c4644ae404e822501e2".to_string();
            app
        })
        .collect::<Vec<_>>();

    environment
}

// echo app environment is an environment that contains http-echo container (forked from hashicorp)
// ECHO_TEXT var will be the content of the application root path
pub fn echo_app_environment(context: &Context, test_domain: &str) -> EnvironmentRequest {
    let suffix = generate_id();
    let application_id = Uuid::new_v4();
    EnvironmentRequest {
        execution_id: context.execution_id().to_string(),
        long_id: application_id,
        name: "env".to_string(),
        project_long_id: Uuid::new_v4(),
        organization_long_id: Uuid::new_v4(),
        action: Action::Create,
        max_parallel_build: 1,
        applications: vec![Application {
            long_id: Uuid::new_v4(),
            name: format!("{}-{}", "echo-app", &suffix),
            /*name: "simple-app".to_string(),*/
            git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
            commit_id: "2205adea1db295547b99f7b17229afd7e879b6ff".to_string(),
            dockerfile_path: Some("Dockerfile".to_string()),
            command_args: vec![],
            entrypoint: None,
            buildpack_language: None,
            root_path: String::from("/"),
            action: Action::Create,
            git_credentials: None,
            storage: vec![],
            environment_vars: btreemap! {
                "ECHO_TEXT".to_string() => base64::encode("42"),
            },
            mounted_files: vec![],
            branch: "echo-app".to_string(),
            ports: vec![Port {
                id: "zdf7d6aad".to_string(),
                long_id: Default::default(),
                port: 5678,
                is_default: true,
                name: None,
                publicly_accessible: true,
                protocol: Protocol::HTTP,
            }],
            total_cpus: "100m".to_string(),
            total_ram_in_mib: 256,
            min_instances: 1,
            max_instances: 1,
            cpu_burst: "100m".to_string(),
            advanced_settings: Default::default(),
        }],
        containers: vec![],
        jobs: vec![],
        routers: vec![Router {
            long_id: Uuid::new_v4(),
            name: "main".to_string(),
            action: Action::Create,
            default_domain: format!("{}.{}.{}", generate_id(), context.cluster_short_id(), test_domain),
            public_port: 443,
            custom_domains: vec![],
            routes: vec![Route {
                path: "/".to_string(),
                service_long_id: application_id,
            }],
        }],
        databases: vec![],
    }
}

pub fn environment_only_http_server(
    context: &Context,
    test_domain: &str,
    with_router: bool,
    with_sticky: bool,
) -> EnvironmentRequest {
    let router_name = "main".to_string();
    let suffix = generate_id();
    let application_id = Uuid::new_v4();
    let application_name = format!("{}-{}", "mini-http", &suffix);
    let application_domain = format!("{}.{}.{}", application_name, context.cluster_short_id(), test_domain);
    let settings = ApplicationAdvancedSettings {
        network_ingress_sticky_session_enable: with_sticky,
        ..Default::default()
    };

    let mut req = EnvironmentRequest {
        execution_id: context.execution_id().to_string(),
        long_id: Uuid::new_v4(),
        name: "env".to_string(),
        project_long_id: Uuid::new_v4(),
        organization_long_id: Uuid::new_v4(),
        action: Action::Create,
        max_parallel_build: 1,
        applications: vec![Application {
            long_id: application_id,
            name: application_name,
            /*name: "simple-app".to_string(),*/
            git_url: "https://github.com/Qovery/engine-testing.git".to_string(),
            commit_id: "d22414a253db2bcf3acf91f85565d2dabe9211cc".to_string(),
            dockerfile_path: Some("Dockerfile".to_string()),
            command_args: vec![],
            entrypoint: None,
            buildpack_language: None,
            root_path: String::from("/"),
            action: Action::Create,
            git_credentials: None,
            storage: vec![],
            environment_vars: BTreeMap::default(),
            mounted_files: vec![],
            branch: "main".to_string(),
            ports: vec![Port {
                id: "zdf7d6aad".to_string(),
                long_id: Default::default(),
                port: 80,
                is_default: true,
                name: None,
                publicly_accessible: true,
                protocol: Protocol::HTTP,
            }],
            total_cpus: "100m".to_string(),
            total_ram_in_mib: 256,
            min_instances: 1,
            max_instances: 1,
            cpu_burst: "100m".to_string(),
            advanced_settings: settings,
        }],
        containers: vec![],
        jobs: vec![],
        routers: vec![],
        databases: vec![],
    };

    if with_router {
        req.routers = vec![Router {
            long_id: Uuid::new_v4(),
            name: router_name,
            action: Action::Create,
            default_domain: application_domain,
            public_port: 443,
            custom_domains: vec![],
            routes: vec![Route {
                path: "/".to_string(),
                service_long_id: application_id,
            }],
        }]
    }

    req
}

pub fn environment_only_http_server_router(context: &Context, test_domain: &str) -> EnvironmentRequest {
    environment_only_http_server(context, test_domain, true, false)
}

pub fn environment_only_http_server_router_with_sticky_session(
    context: &Context,
    test_domain: &str,
) -> EnvironmentRequest {
    environment_only_http_server(context, test_domain, true, true)
}

/// Test if stick sessions are activated on given routers via cookie.
pub fn session_is_sticky(url: Url, host: String, max_age: u32) -> bool {
    let mut is_ok = true;
    let http_client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true) // this test ignores certificate validity (not its purpose)
        .build()
        .expect("Cannot build reqwest client");

    let http_request_result = http_client.get(url.to_string()).header("Host", host.as_str()).send();

    if let Err(e) = http_request_result {
        error!("Unable to get {} with host '{}': {}", url, host, e);
        return false;
    }

    let http_response = http_request_result.expect("cannot retrieve HTTP request result");

    is_ok &= match http_response.headers().get("Set-Cookie") {
        None => {
            error!("Unable to get http response 'Set-Cookie' header");
            false
        }
        Some(value) => match value.to_str() {
            Err(_) => {
                error!("Unable to parse {:?}", value);
                false
            }
            Ok(s) => s.contains("INGRESSCOOKIE_QOVERY=") && s.contains(format!("Max-Age={max_age}").as_str()),
        },
    };

    is_ok
}
