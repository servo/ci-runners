usage
=====

This program analyses usage efficiency and value for money.

```
$ ./download-logs.sh ci{0..4}.servo.org
$ cargo run -r -- ci{0..4}.servo.org.log
```

## 2025-09-24

```
$ cargo run -r -- ci{0..2}.servo.org.log
```
### ci0.servo.org.log
Over the last PT4134359.863421S (47.85 days) of uptime:
- 14 runners in profile servo-macos13:
    - Busy for 0.25%, PT10293.37245S (0.12 days)
    - DoneOrUnregistered for 0.00%, PT74.156025S (0.00 days)
    - Idle for 1.06%, PT43936.083203S (0.51 days)
    - Reserved for 0.01%, PT297.483276S (0.00 days)
    - StartedOrCrashed for 0.01%, PT580.52404S (0.01 days)
- 2956 runners in profile servo-ubuntu2204:
    - Busy for 11.89%, PT491583.851343S (5.69 days)
    - DoneOrUnregistered for 0.13%, PT5172.208085S (0.06 days)
    - Idle for 178.89%, PT7396007.395667S (85.60 days)
    - Invalid for 0.00%, PT25.612676S (0.00 days)
    - Reserved for 0.50%, PT20800.647281S (0.24 days)
    - StartedOrCrashed for 6.24%, PT257963.814314S (2.99 days)
- 3768 runners in profile servo-windows10:
    - Busy for 26.32%, PT1088329.166799S (12.60 days)
    - DoneOrUnregistered for 0.23%, PT9603.388518S (0.11 days)
    - Idle for 163.43%, PT6756947.0056S (78.21 days)
    - Invalid for 0.00%, PT126.549871S (0.00 days)
    - Reserved for 1.09%, PT44910.582155S (0.52 days)
    - StartedOrCrashed for 7.02%, PT290195.324798S (3.36 days)
### ci1.servo.org.log
Over the last PT4226954.94317S (48.92 days) of uptime:
- 2484 runners in profile servo-macos13:
    - Busy for 12.75%, PT539036.593032S (6.24 days)
    - DoneOrUnregistered for 0.11%, PT4855.302898S (0.06 days)
    - Idle for 78.29%, PT3309280.06831S (38.30 days)
    - Invalid for 0.00%, PT6.455368S (0.00 days)
    - Reserved for 0.58%, PT24426.983473S (0.28 days)
    - StartedOrCrashed for 5.21%, PT220360.094476S (2.55 days)
- 1 runners in profile servo-macos15:
    - Idle for 1.91%, PT80819.344573S (0.94 days)
    - StartedOrCrashed for 0.00%, PT30.969065S (0.00 days)
- 4006 runners in profile servo-ubuntu2204:
    - Busy for 9.42%, PT398043.558698S (4.61 days)
    - DoneOrUnregistered for 0.12%, PT5235.43797S (0.06 days)
    - Idle for 279.83%, PT11828441.455651S (136.90 days)
    - Invalid for 0.00%, PT105.205117S (0.00 days)
    - Reserved for 0.58%, PT24495.749191S (0.28 days)
    - StartedOrCrashed for 8.59%, PT363236.06395S (4.20 days)
### ci2.servo.org.log
Over the last PT3011011.207145S (34.85 days) of uptime:
- 2097 runners in profile servo-macos13:
    - Busy for 12.62%, PT380081.255097S (4.40 days)
    - DoneOrUnregistered for 0.11%, PT3331.050649S (0.04 days)
    - Idle for 75.87%, PT2284514.258751S (26.44 days)
    - Invalid for 0.00%, PT6.131445S (0.00 days)
    - Reserved for 0.54%, PT16293.936156S (0.19 days)
    - StartedOrCrashed for 6.50%, PT195603.919187S (2.26 days)
- 2 runners in profile servo-macos14:
    - Busy for 0.01%, PT263.49627S (0.00 days)
    - DoneOrUnregistered for 0.00%, PT6.093588S (0.00 days)
    - Idle for 2.75%, PT82719.086562S (0.96 days)
    - Reserved for 0.00%, PT33.196788S (0.00 days)
    - StartedOrCrashed for 0.00%, PT62.842819S (0.00 days)
- 3850 runners in profile servo-ubuntu2204:
    - Busy for 11.45%, PT344701.85398S (3.99 days)
    - DoneOrUnregistered for 0.15%, PT4440.240026S (0.05 days)
    - Idle for 273.90%, PT8247061.653337S (95.45 days)
    - Invalid for 0.00%, PT30.747625S (0.00 days)
    - Reserved for 0.68%, PT20479.706976S (0.24 days)
    - StartedOrCrashed for 12.09%, PT363897.976352S (4.21 days)
### Monthly usage (per month of 30 days)
Runner hours spent in Busy, scaled to 30 days:
- servo-macos13: PT664183.847270946S (7.69 days)
- servo-macos14: PT226.828226417S (0.00 days)
- servo-ubuntu2204: PT849010.62844953S (9.83 days)
- servo-windows10: PT682318.253256453S (7.90 days)
### Equivalent spend (per month of 30 days)
NOTE: this doesn’t even consider the speedup vs free runners!
- servo-macos13:
    - Namespace macOS arm64 5cpu:
      7.69 days/month × 91.53 EUR/day = 703.64 EUR/month
    - WarpBuild macOS arm64 6cpu:
      7.69 days/month × 97.64 EUR/day = 750.55 EUR/month
    - GitHub macOS arm64 5cpu:
      7.69 days/month × 195.27 EUR/day = 1501.11 EUR/month
- servo-ubuntu2204:
    - Namespace Linux x64 8cpu:
      9.83 days/month × 14.65 EUR/day = 143.91 EUR/month
    - WarpBuild Linux arm64 8cpu:
      9.83 days/month × 14.65 EUR/day = 143.91 EUR/month
    - WarpBuild Linux x64 8cpu:
      9.83 days/month × 19.53 EUR/day = 191.88 EUR/month
    - GitHub Linux x64 8cpu:
      9.83 days/month × 39.05 EUR/day = 383.77 EUR/month
- servo-windows10:
    - Namespace Windows x64 8cpu:
      7.90 days/month × 29.29 EUR/day = 231.31 EUR/month
    - WarpBuild Linux x64 8cpu:
      7.90 days/month × 39.05 EUR/day = 308.42 EUR/month
    - GitHub Windows x64 8cpu:
      7.90 days/month × 78.11 EUR/day = 616.84 EUR/month
