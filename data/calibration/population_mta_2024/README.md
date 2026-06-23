# MTA Population Calibration Fixture

`times_sq_2024_12_31_hourly.csv` is a pinned station-hour excerpt from the
New York State Open Data MTA Subway Hourly Ridership 2020-2024 dataset:

<https://data.ny.gov/Transportation/MTA-Subway-Hourly-Ridership-2020-2024/wujg-7c2s>

The fixture uses the Times Sq-42 St / 42 St station complex for December 31,
2024. Rows were aggregated across fare-payment classes with the Socrata query:

```sql
select transit_timestamp, station_complex, sum(ridership)
where station_complex = 'Times Sq-42 St (N,Q,R,W,S,1,2,3,7)/42 St (A,C,E)'
  and transit_timestamp between '2024-12-31T00:00:00' and '2025-01-01T00:00:00'
group by transit_timestamp, station_complex
order by transit_timestamp
```

The `ridership` field is an hourly station-complex ridership estimate, not a
platform occupancy count or an exit-flow observation.
