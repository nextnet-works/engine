use crate::helpers::utilities::{context_for_ec2, engine_run_test, generate_organization_id, logger, FuncTestsSecrets};
use ::function_name::named;
use qovery_engine::cloud_provider::kubernetes::Kind as KKind;
use std::str::FromStr;

use crate::helpers::common::ClusterDomain;
use crate::helpers::kubernetes::{cluster_test, ClusterTestType};
use crate::helpers::utilities::generate_cluster_id;
use qovery_engine::cloud_provider::aws::kubernetes::VpcQoveryNetworkMode;
use qovery_engine::cloud_provider::aws::kubernetes::VpcQoveryNetworkMode::WithoutNatGateways;
use qovery_engine::cloud_provider::aws::regions::AwsRegion;
use qovery_engine::cloud_provider::models::CpuArchitecture;
use qovery_engine::cloud_provider::Kind;
use qovery_engine::utilities::to_short_id;

fn create_and_destroy_aws_ec2_k3s_cluster(
    test_type: ClusterTestType,
    vpc_network_mode: VpcQoveryNetworkMode,
    test_name: &str,
) {
    engine_run_test(|| {
        let secrets = FuncTestsSecrets::new();

        let localisation = AwsRegion::from_str(
            secrets
                .AWS_EC2_TEST_INSTANCE_REGION
                .expect("AWS_EC2_TEST_INSTANCE_REGION is not set")
                .as_str(),
        )
        .expect("Invalid AWS region");
        let zones = localisation.get_zones();
        let cluster_id = generate_cluster_id(localisation.to_aws_format());
        let organization_id = generate_organization_id(localisation.to_aws_format());
        cluster_test(
            test_name,
            Kind::Aws,
            KKind::Ec2,
            context_for_ec2(organization_id, cluster_id),
            logger(),
            localisation.to_aws_format(),
            Some(zones),
            test_type,
            &ClusterDomain::Default {
                cluster_id: to_short_id(&cluster_id),
            },
            Option::from(vpc_network_mode),
            CpuArchitecture::AMD64,
            None,
        )
    })
}

#[cfg(feature = "test-aws-ec2-infra")]
#[named]
#[test]
fn create_and_destroy_aws_ec2_k3s_cluster_eu_west_1() {
    create_and_destroy_aws_ec2_k3s_cluster(ClusterTestType::Classic, WithoutNatGateways, function_name!());
}
