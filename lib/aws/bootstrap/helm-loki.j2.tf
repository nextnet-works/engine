resource "aws_iam_user" "iam_eks_loki" {
  name = "qovery-logs-${var.kubernetes_cluster_id}"
  tags = local.tags_eks
}

resource "aws_iam_access_key" "iam_eks_loki" {
  user    = aws_iam_user.iam_eks_loki.name
}

resource "aws_iam_policy" "loki_s3_policy" {
  name = aws_iam_user.iam_eks_loki.name
  description = "Policy for logs storage"

  policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": "s3:*",
            "Resource": "*"
        }
    ]
}
POLICY
}

resource "aws_iam_user_policy_attachment" "s3_loki_attachment" {
  user       = aws_iam_user.iam_eks_loki.name
  policy_arn = aws_iam_policy.loki_s3_policy.arn
}

resource "aws_kms_key" "s3_logs_kms_encryption" {
  description             = "s3 logs encryption"
  tags = merge(
    local.tags_eks,
    {
      "Name" = "Encryption logs"
    }
  )
}

resource "aws_s3_bucket_server_side_encryption_configuration" "lok_bucket_enryption" {
  bucket = aws_s3_bucket.loki_bucket.id

  rule {
    apply_server_side_encryption_by_default {
      kms_master_key_id = aws_kms_key.s3_logs_kms_encryption.arn
      sse_algorithm = "aws:kms"
    }
  }
}

// S3 bucket to store indexes and logs
resource "aws_s3_bucket" "loki_bucket" {
  bucket = aws_iam_user.iam_eks_loki.name
  force_destroy = true

  tags = merge(
    local.tags_eks,
    {
      {% if is_deletion_step %}
      "can_be_deleted_by_owner" = "true"
      {% endif %}
      "Name" = "Applications logs"
    }
  )
}

resource "aws_s3_bucket_lifecycle_configuration" "loki_lifecycle" {
  bucket = aws_s3_bucket.loki_bucket.id
  rule {
    id = "on_delete_rule"

    expiration {
      days = 1
      expired_object_delete_marker = true
    }

    noncurrent_version_expiration {
      noncurrent_days = 1
    }

    {% if is_deletion_step %}
    status = "Enabled"
    {% else %}
    status = "Disabled"
    {% endif %}
  }

}

resource "aws_s3_bucket_versioning" "loki_bucket_versioning" {
  bucket = aws_s3_bucket.loki_bucket.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_ownership_controls" "loki_bucket_ownership" {
  bucket = aws_s3_bucket.loki_bucket.id
  rule {
    object_ownership = "ObjectWriter"
  }
}

resource "aws_s3_bucket_acl" "loki_bucket_acl" {
  bucket = aws_s3_bucket.loki_bucket.id
  acl    = "private"

  depends_on = [
    aws_s3_bucket_ownership_controls.loki_bucket_ownership,
    aws_s3_bucket_public_access_block.loki_access,
  ]
}

resource "aws_s3_bucket_public_access_block" "loki_access" {
  bucket = aws_s3_bucket.loki_bucket.id

  ignore_public_acls = true
  restrict_public_buckets  = true
  block_public_policy = true
  block_public_acls = true
}