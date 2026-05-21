# sezar-id V4.2 — AWS KMS bring-up

`sezar-id aws-kms-scan` lists every customer-managed key
(CMK) in a region via AWS KMS `ListKeys` + `DescribeKey`,
maps the `KeySpec` onto the shared `Primitive` table the
offline + PKCS#11 paths use, and emits one
`crypto_inventory_event` per CMK with
`asset.kind = hsm_slot`.

## Host requirements

| Requirement       | Why                                                                  |
|-------------------|----------------------------------------------------------------------|
| AWS credentials   | Standard SDK resolution: env (`AWS_*`), shared config, IMDS.         |
| `kms:ListKeys`    | IAM permission on the role / user the credentials resolve to.        |
| `kms:DescribeKey` | Same; the scanner runs one DescribeKey per ListKeys result.          |

A minimal IAM policy:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["kms:ListKeys", "kms:DescribeKey"],
      "Resource": "*"
    }
  ]
}
```

## Bring-up

```bash
# 1. Build with the aws-kms feature.
cargo build --release -p sezar-id --features aws-kms

# 2. Auth via the standard SDK chain. Easiest in
#    development is `aws sso login` + the shared config;
#    in production prefer IAM Roles for Service Accounts
#    (EKS) or instance-profile credentials.
export AWS_REGION=us-east-1
# or aws configure / aws sso login / etc.

# 3. Scan.
./target/release/sezar-id aws-kms-scan \
    --region us-east-1 \
    --collector http://127.0.0.1:8090/v1/events
```

Expected output (logged to stderr):

```
[sezar_id] aws-kms-scan starting backend="aws-kms"
[sezar_id] aws-kms-scan complete keys_seen=12 events_emitted=12
```

`GET /v1/inventory` then returns 12 `hsm_slot` rows, each
with `host: "aws-kms:us-east-1"` and the right primitive
mapping (RSA-2048/3072/4096, ECDSA-P256/384/521, ECDSA-
secp256k1, AES-256, HMAC-SHA256/384/512).

## What about GCP KMS / Azure Key Vault?

The `KmsBackend` trait stays narrow on purpose:

```rust
#[async_trait]
pub trait KmsBackend: Send + Sync {
    async fn list_keys(&self) -> Result<Vec<KmsKeyInfo>>;
    fn backend_label(&self) -> &'static str;
}
```

Adding GCP KMS or Azure Key Vault is one impl per provider
behind its own feature flag (`gcp-kms`, `azure-key-vault`).
The scanner loop in [`crate::aws_kms::kms_scan`] stays
unchanged — it walks a `Vec<KmsKeyInfo>` regardless of
provider.

V4.2 ships the AWS backend; V4.x adds the other two when
operators ask.

## Multi-region

The current CLI scans one region per invocation. Loop the
command over `us-east-1`, `eu-west-1`, … in a wrapper
script, or extend the binary with a `--region <r>
--region <r>` multi-arg in a follow-up — both work.

## Troubleshooting

**`UnrecognizedClientException`.** Credentials aren't
resolving. Run `aws sts get-caller-identity` to confirm
the SDK chain works outside sezar-id.

**`AccessDeniedException` on `ListKeys`.** The IAM role
lacks the `kms:ListKeys` permission listed above.

**Some keys missing from the scan output.** AWS KMS
returns *enabled* keys by default; pending-deletion and
disabled keys show as separate `KeyState` values. The
current scanner doesn't filter — every key the API returns
becomes an event. If your inventory looks short, check
`KeyState` on the missing keys via the AWS console.
