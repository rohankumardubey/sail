use std::sync::Arc;

use datafusion::functions::expr_fn;
use datafusion_common::ScalarValue;
use datafusion_expr::{expr, ScalarUDF};

use crate::error::PlanResult;
use crate::extension::function::math::spark_hex_unhex::SparkHex;
use crate::extension::function::spark_crc32::SparkCrc32;
use crate::extension::function::spark_murmur3_hash::SparkMurmur3Hash;
use crate::extension::function::spark_sha1::SparkSha1;
use crate::extension::function::spark_xxhash64::SparkXxhash64;
use crate::function::common::{ScalarFunction, ScalarFunctionInput};
use crate::utils::ItemTaker;

fn sha2(input: ScalarFunctionInput) -> PlanResult<expr::Expr> {
    let ScalarFunctionInput { arguments, .. } = input;
    let (expr, bit_length) = arguments.two()?;
    let result = match bit_length {
        expr::Expr::Literal(ScalarValue::Int32(Some(0)), _metadata)
        | expr::Expr::Literal(ScalarValue::Int32(Some(256)), _metadata) => expr_fn::sha256(expr),
        expr::Expr::Literal(ScalarValue::Int32(Some(224)), _metadata) => expr_fn::sha224(expr),
        expr::Expr::Literal(ScalarValue::Int32(Some(384)), _metadata) => expr_fn::sha384(expr),
        expr::Expr::Literal(ScalarValue::Int32(Some(512)), _metadata) => expr_fn::sha512(expr),
        _ => {
            return Err(crate::error::PlanError::invalid(
                "The second argument of sha2 must be a literal integer.",
            ))
        }
    };
    let hex = expr::Expr::ScalarFunction(expr::ScalarFunction {
        func: Arc::new(ScalarUDF::from(SparkHex::new())),
        args: vec![result],
    });
    Ok(expr_fn::lower(hex))
}

pub(super) fn list_built_in_hash_functions() -> Vec<(&'static str, ScalarFunction)> {
    use crate::function::common::ScalarFunctionBuilder as F;

    vec![
        ("crc32", F::udf(SparkCrc32::new())),
        ("hash", F::udf(SparkMurmur3Hash::new())),
        ("md5", F::unary(expr_fn::md5)),
        ("sha", F::udf(SparkSha1::new())),
        ("sha1", F::udf(SparkSha1::new())),
        ("sha2", F::custom(sha2)),
        ("xxhash64", F::udf(SparkXxhash64::new())),
    ]
}
