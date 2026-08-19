Archival climate records (uds §14.14.1), and what SWMM 5 itself makes of
them. Each was run through the reference implementation with
`[TEMPERATURE] FILE` and `[EVAPORATION] FILE`, and the values below are
what its binary output carried in the system air-temperature and
potential-evaporation series. They are the expectations in
`io::climate::archive_tests`.

The oracle is indirect because SWMM never writes a climate record back
out. Air temperature is reported as an hourly sinusoid between the day's
two extremes, so a run brackets the values it read rather than reproducing
them, and the daily envelope is quoted below for that reason. Potential
evaporation is the file's own value unchanged, so it pins that column
exactly.

user.dat        the user-prepared format (§14.14), the reference run
  2024-01-15  tmax 55 F  tmin 33 F  evap 0.20 in  wind 7 mph
  2024-01-16  tmax 61 F  tmin 39 F  evap 0.30 in  wind 9 mph
  envelope 33.53..54.47 then 39.37..60.50; PET 0.200 then 0.300

td3200.dat      NCDC TD-3200, the same two days
  Reproduces the reference run exactly: identical envelope, identical
  PET. Temperatures are whole degrees F, evaporation hundredths of an
  inch (20 and 30).

td3200_missing.dat   99999 on the second day
  Day 2 holds day 1's values: envelope 33.50..54.50. A missing reading
  is not a zero one.

td3200_badflag.dat   the second day's TMAX flagged '9'
  Only TMAX is dropped, and only for that day: envelope 39.37..54.63,
  the minimum having moved to 39 while the maximum held at 55. This is
  the flag rule on its own, isolated from everything else.

dly0204.dat     Environment-Canada DLY02/DLY04, element codes 1, 2, 151
  Temperatures in tenths of a degree Celsius (128, 150 and 6, 39) and
  evaporation in tenths of a millimetre (50, 76).
  envelope 33.60..54.52 then 39.39..58.54
  PET 0.19685 then 0.29921, which is 5.0 mm and 7.6 mm exactly.

dly0204_blank.dat    a blank value field rather than 99999
  Also missing: day 2 holds day 1's, envelope 33.58..54.54.

ghcnd.dat       NCDC GHCN-Daily, header-positioned columns
  The same two days in degrees F and inches, and it reproduces the
  reference run exactly.

ghcnd_c.dat     the same days declared in Celsius on the INP line
  12.8 and 15.0, 0.6 and 3.9, read with a `C` units word. The envelope
  matches the Canadian file's, 33.60..54.52, because they are the same
  temperatures written two ways.

ghcnd_missing.dat    9999 on the second day
  Missing in this layout is a magnitude of 9999 or more rather than a
  sentinel string. Day 2 holds day 1's values: envelope 33.50..54.50.

td3200_negative.dat  a below-freezing minimum
  The sign is its own column, ahead of the digits. tmax 20 F and tmin
  -5 F, then 25 and -10: envelope -4.40..19.40 then -9.31..24.20.

dly0204_negative.dat  the same, in tenths of a degree Celsius
  12.8 C and -5.5 C: envelope 22.89..54.25, which is 55.04 and 22.10 F
  approached from inside.

unknown.dat     a file in none of these layouts
  SWMM refuses it: "ERROR 338: error in reading from climate file".
  A file that is recognised but misaligned is not refused there, which
  is the deviation §14.14.1 records.

Wind has no oracle here. SWMM reports air temperature and potential
evaporation as system series and reports wind nowhere, so the wind
columns are asserted against the conversions §14.14 states rather than
against a run. They sit in the same groups as the quantities above, in
every layout, so their *positions* are pinned by the fixtures that are
verified; only the unit factors are not.
