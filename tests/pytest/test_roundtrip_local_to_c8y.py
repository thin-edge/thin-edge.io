
import sys
from datetime import datetime, timezone

# Run :
# pytest-3 --capture no tests/pytest/test_roundtrip_local_to_c8y.py

sys.path.append("ci")
from roundtrip_local_to_c8y import check_timestamps, is_timezone_aware


def test_check_timestamps_increasing_ms():
    timestamps = ['2021-01-01T01:00:00.001+00:00',
                  '2021-01-01T01:00:00.002+00:00',
                  '2021-01-01T01:00:00.003+00:00',
                  '2021-01-01T01:00:00.004+00:00',
                  '2021-01-01T01:00:00.005+00:00']
    laststamp = datetime(2021, 1, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    ret = check_timestamps(timestamps, laststamp)
    assert ret == True

def test_is_timezone_aware():
    stamp = datetime.fromisoformat('2021-01-01T01:00:00.001+00:00')
    assert is_timezone_aware(stamp) == True

    stamp = datetime.fromisoformat('2021-01-01T01:00:00.000+00:00')
    assert is_timezone_aware(stamp) == True

    #TODO: needs: https://dateutil.readthedocs.io/en/stable/parser.html#dateutil.parser.isoparse
    #stamp = datetime.fromisoformat('2021-01-01T01:00:00.000Z')
    #assert is_timezone_aware(stamp) == True

    stamp = datetime.fromisoformat('2021-01-01T01:00:00.000')
    assert is_timezone_aware(stamp) == False
