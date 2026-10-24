#ifndef FASTLANG_CLIB_TIME_H
#define FASTLANG_CLIB_TIME_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int32_t fast_time_get_year(void);
int32_t fast_time_get_month(void);
int32_t fast_time_get_day(void);
int32_t fast_time_get_day_of_week(void);
int32_t fast_time_get_hour(void);
int32_t fast_time_get_minute(void);
int32_t fast_time_get_second(void);
int32_t fast_time_get_millisecond(void);

int64_t fast_time_steady_ms(void);
int64_t fast_time_steady_ns(void);
int64_t fast_time_epoch_ms(void);
void fast_time_sleep_ms(int64_t ms);

#ifdef __cplusplus
}
#endif

#endif // FASTLANG_CLIB_TIME_H
