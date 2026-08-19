Archival station records (uds §14.12.1), and what SWMM 5 itself makes of
them. Each was run through the reference implementation with a gage
reading it and `SAVE RAINFALL`, and the readings below are what the
resulting interface file held. They are the expectations in
`io::rain::archive_tests`.

nws_space.dat   NWS space-delimited, hourly (HPCP)
  00:00 = 0.25 in, 01:00 = 0.10 in
  The 03:00 reading is flagged M and the 04:00 reading is zero; neither
  reaches the file. Every reading is stamped one interval before the
  instant the record marks, because the record marks the end.

nws_comma.dat   the same record, comma-delimited
nws_tape.dat    the same record, fixed-field tape
  Both produce the identical two readings.

nws_accum.dat   an accumulation period
  `a` at 01:00 opens it, `A` at 04:00 closes it with 0.60 in, and the
  total is divided evenly over the four intervals from 01:00 to 04:00
  inclusive: 0.15 in each at 00:00, 01:00, 02:00 and 03:00. A separate
  0.05 in reading at 06:00 lands at 05:00.
