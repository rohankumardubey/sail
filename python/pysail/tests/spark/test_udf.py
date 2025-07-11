import pytest
from pyspark import Row
from pyspark.sql.functions import udf


@pytest.fixture(params=[True, False], ids=["arrow", "non-arrow"])
def arrow(spark, request):
    # The PySpark UDF Arrow mode is buggy since it may produce results
    # different from the non-Arrow mode (e.g. type casting support).
    # Some of the issues are not seen in Sail, so we can write tests and
    # make sure that both modes produce the same results.
    value = "true" if request.param else "false"
    spark.conf.set("spark.sql.execution.pythonUDF.arrow.enabled", value)
    yield
    spark.conf.unset("spark.sql.execution.pythonUDF.arrow.enabled")


@pytest.mark.usefixtures("arrow")
@pytest.mark.skip(reason="not working")
def test_implicit_string_casting(spark):
    # The default UDF return type is "string".
    df = spark.sql("SELECT 1 as a").select(udf(lambda x: x)("a").alias("b"))
    assert df.collect() == [Row(b="1")]

    df = spark.sql("SELECT 1 as a").select(udf(lambda x: [x], returnType="array<string>")("a").alias("b"))
    assert df.collect() == [Row(b=["1"])]


@pytest.mark.usefixtures("arrow")
@pytest.mark.skip(reason="not working")
def test_implicit_binary_casting_invalid_type(spark):
    df = spark.sql("SELECT 1 as a").select(udf(lambda x: x, returnType="binary")("a").alias("b"))
    assert df.collect() == [Row(b=None)]

    df = spark.sql("SELECT 1 as a").select(udf(lambda x: [x], returnType="array<binary>")("a").alias("b"))
    assert df.collect() == [Row(b=[None])]


@pytest.mark.usefixtures("arrow")
@pytest.mark.skip(reason="not working")
def test_implicit_binary_casting_string_type(spark):
    df = spark.sql("SELECT '1' as a").select(udf(lambda x: x, returnType="binary")("a").alias("b"))
    assert df.collect() == [Row(b=bytearray(b"1"))]

    df = spark.sql("SELECT '1' as a").select(udf(lambda x: [x], returnType="array<binary>")("a").alias("b"))
    assert df.collect() == [Row(b=[bytearray(b"1")])]
