usage analysis
==============

This program analyses usage efficiency and value for money.

```
$ ./download-logs.sh ci{0..4}.servo.org
$ cargo run -r -- ci{0..4}.servo.org.log
```

## 2025-12-02

efficiency of utilisation has regressed, or at best not improved, for servo-windows10 (9.66% vs 10.75%), servo-ubuntu2204 (6.26% vs 6.34%), and servo-ubuntu2204-bench (1.65% vs 1.93%).

relevant changes: [#69](https://github.com/servo/ci-runners/pull/69)), [servo#40852](https://github.com/servo/servo/pull/40852), [servo#40876](https://github.com/servo/servo/pull/40876).

```
$ ./download-logs.sh ci{0..4}.servo.org -- -S '2025-11-25 05:07'
$ cargo run -r -- ci{0..4}.servo.org.log
```

### ci0.servo.org.log
Over the last PT610273.819056S (7.06 days) of uptime:
- 387 runners in profile servo-ubuntu2204:
    - Busy for 18.94%, PT115571.951782S (1.34 days)
    - DoneOrUnregistered for 0.37%, PT2254.151424S (0.03 days)
    - Idle for 276.26%, PT1685935.924589S (19.51 days)
    - Reserved for 1.79%, PT10926.1131S (0.13 days)
    - StartedOrCrashed for 1.49%, PT9117.517271S (0.11 days)
- 98 runners in profile servo-windows10:
    - Busy for 9.41%, PT57409.009004S (0.66 days)
    - DoneOrUnregistered for 0.09%, PT565.033875S (0.01 days)
    - Idle for 89.25%, PT544653.101598S (6.30 days)
    - Reserved for 0.43%, PT2608.416035S (0.03 days)
    - StartedOrCrashed for 0.43%, PT2602.306119S (0.03 days)
### ci1.servo.org.log
Over the last PT610294.964296S (7.06 days) of uptime:
- 388 runners in profile servo-ubuntu2204:
    - Busy for 18.36%, PT112028.020681S (1.30 days)
    - DoneOrUnregistered for 0.37%, PT2281.925353S (0.03 days)
    - Idle for 277.02%, PT1690619.778802S (19.57 days)
    - Reserved for 1.79%, PT10933.962459S (0.13 days)
    - StartedOrCrashed for 1.49%, PT9093.451063S (0.11 days)
- 98 runners in profile servo-windows10:
    - Busy for 9.75%, PT59522.149024S (0.69 days)
    - DoneOrUnregistered for 0.09%, PT556.275635S (0.01 days)
    - Idle for 88.91%, PT542596.842428S (6.28 days)
    - Reserved for 0.42%, PT2544.313662S (0.03 days)
    - StartedOrCrashed for 0.44%, PT2682.920329S (0.03 days)
### ci2.servo.org.log
Over the last PT610305.424532S (7.06 days) of uptime:
- 415 runners in profile servo-ubuntu2204:
    - Busy for 18.81%, PT114804.581516S (1.33 days)
    - DoneOrUnregistered for 0.40%, PT2423.802561S (0.03 days)
    - Idle for 276.22%, PT1685814.436006S (19.51 days)
    - Reserved for 1.89%, PT11550.187268S (0.13 days)
    - StartedOrCrashed for 1.60%, PT9789.914209S (0.11 days)
- 95 runners in profile servo-windows10:
    - Busy for 9.71%, PT59288.488976S (0.69 days)
    - DoneOrUnregistered for 0.09%, PT541.916269S (0.01 days)
    - Idle for 88.98%, PT543037.425422S (6.29 days)
    - Reserved for 0.42%, PT2533.847903S (0.03 days)
    - StartedOrCrashed for 0.42%, PT2575.017529S (0.03 days)
### ci3.servo.org.log
Over the last PT610572.88725S (7.07 days) of uptime:
- 42 runners in profile servo-ubuntu2204-bench:
    - Busy for 1.55%, PT9447.008115S (0.11 days)
    - DoneOrUnregistered for 0.04%, PT221.04072S (0.00 days)
    - Idle for 97.90%, PT597727.593627S (6.92 days)
    - Reserved for 0.19%, PT1180.842532S (0.01 days)
    - StartedOrCrashed for 0.15%, PT945.744675S (0.01 days)
### ci4.servo.org.log
Over the last PT610558.342957S (7.07 days) of uptime:
- 46 runners in profile servo-ubuntu2204-bench:
    - Busy for 1.76%, PT10722.614157S (0.12 days)
    - DoneOrUnregistered for 0.04%, PT249.259018S (0.00 days)
    - Idle for 97.63%, PT596064.720228S (6.90 days)
    - Reserved for 0.22%, PT1336.592489S (0.02 days)
    - StartedOrCrashed for 0.16%, PT971.289351S (0.01 days)
### Efficiency of utilisation (Busy time / total time)
Independent of runner concurrency:
- servo-ubuntu2204: 6.26% (PT342404.553979S / PT5473145.718084S)
- servo-ubuntu2204-bench: 1.65% (PT20169.622272S / PT1218866.704912S)
- servo-windows10: 9.66% (PT176219.647004S / PT1823717.063808S)
### Monthly usage (per month of 30 days)
Runner hours spent in Busy, scaled to 30 days:
- servo-ubuntu2204: PT1454244.152573057S (16.83 days)
- servo-ubuntu2204-bench: PT85625.031976511S (0.99 days)
- servo-windows10: PT748431.289353075S (8.66 days)
### Equivalent spend (per month of 30 days)
NOTE: this doesn’t even consider the speedup vs free runners!
- servo-macos13:
- servo-ubuntu2204:
    - Namespace Linux x64 8cpu:
      16.83 days/month × 14.65 EUR/day = 246.50 EUR/month
    - WarpBuild Linux arm64 8cpu:
      16.83 days/month × 14.65 EUR/day = 246.50 EUR/month
    - WarpBuild Linux x64 8cpu:
      16.83 days/month × 19.53 EUR/day = 328.67 EUR/month
    - GitHub Linux x64 8cpu:
      16.83 days/month × 39.05 EUR/day = 657.34 EUR/month
- servo-windows10:
    - Namespace Windows x64 8cpu:
      8.66 days/month × 29.29 EUR/day = 253.73 EUR/month
    - WarpBuild Linux x64 8cpu:
      8.66 days/month × 39.05 EUR/day = 338.30 EUR/month
    - GitHub Windows x64 8cpu:
      8.66 days/month × 78.11 EUR/day = 676.61 EUR/month

## Past reports

- [2025-11-04](2025-11-04.md)
- [2025-10-27](2025-10-27.md)
- [2025-09-24](2025-09-24.md)
