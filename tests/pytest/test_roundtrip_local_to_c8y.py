import sys
from datetime import datetime, timezone

# Run :
# pytest-3 --capture no tests/pytest/test_roundtrip_local_to_c8y.py

sys.path.append("ci")
from roundtrip_local_to_c8y import check_timestamps, is_timezone_aware


def test_check_timestamps_increasing_ms_timezone_aware():
    timestamps = [
        "2021-01-01T01:00:00.001+00:00",
        "2021-01-01T01:00:00.002+00:00",
        "2021-01-01T01:00:00.003+00:00",
        "2021-01-01T01:00:00.004+00:00",
        "2021-01-01T01:00:00.005+00:00",
    ]
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is True


def test_check_timestamps_increasing_timezone_naive():
    timestamps = [
        "2021-01-01T01:00:00.001Z",
        "2021-01-01T01:00:00.002Z",
        "2021-01-01T01:00:00.003Z",
        "2021-01-01T01:00:00.004Z",
        "2021-01-01T01:00:00.005Z",
    ]
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is True


def test_check_timestamps_wrong_order():
    timestamps = ["2021-01-01T01:00:00.002Z", "2021-01-01T01:00:00.001Z"]
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is False


def test_check_timestamps_equal():
    timestamps = ["2021-01-01T01:00:00.002Z", "2021-01-01T01:00:00.002Z"]
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is True


def test_check_timestamps_equal_tz_aware():
    timestamps = ["2021-01-01T01:00:00.002+00:00", "2021-01-01T01:00:00.002+00:00"]
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is True


def test_check_timestamps_tz_aware_different_timezone():
    timestamps = ["2021-01-01T03:00:00.002+02:00", "2021-01-01T03:00:00.003+02:00"]
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is True


def test_check_timestamps_too_early():
    timestamps = ["2021-01-01T01:00:00.002Z", "2021-01-01T01:00:00.002Z"]
    laststamp = datetime(2021, 1, 1, 2, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret is False


def test_is_timezone_aware():
    stamp = datetime.fromisoformat("2021-01-01T01:00:00.001+00:00")
    assert is_timezone_aware(stamp) is True

    stamp = datetime.fromisoformat("2021-01-01T01:00:00.000+00:00")
    assert is_timezone_aware(stamp) is True

    # TODO: needs: https://dateutil.readthedocs.io/en/stable/parser.html#dateutil.parser.isoparse
    # stamp = datetime.fromisoformat('2021-01-01T01:00:00.000Z')
    # assert is_timezone_aware(stamp) is True

    stamp = datetime.fromisoformat("2021-01-01T01:00:00.000")
    assert is_timezone_aware(stamp) is False
