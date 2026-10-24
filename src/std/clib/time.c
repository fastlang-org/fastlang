#include "time.h"

#if defined(_WIN32) || defined(_WIN64)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

int32_t fast_time_get_year(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wYear;
}

int32_t fast_time_get_month(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wMonth;
}

int32_t fast_time_get_day(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wDay;
}

int32_t fast_time_get_day_of_week(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wDayOfWeek;
}

int32_t fast_time_get_hour(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wHour;
}

int32_t fast_time_get_minute(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wMinute;
}

int32_t fast_time_get_second(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wSecond;
}

int32_t fast_time_get_millisecond(void) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    return (int32_t)st.wMilliseconds;
}

int64_t fast_time_steady_ms(void) {
    static LARGE_INTEGER freq;
    static int initialized = 0;
    if (!initialized) {
        QueryPerformanceFrequency(&freq);
        initialized = 1;
    }
    LARGE_INTEGER counter;
    QueryPerformanceCounter(&counter);
    return (int64_t)((counter.QuadPart * 1000) / freq.QuadPart);
}

int64_t fast_time_steady_ns(void) {
    static LARGE_INTEGER freq;
    static int initialized = 0;
    if (!initialized) {
        QueryPerformanceFrequency(&freq);
        initialized = 1;
    }
    LARGE_INTEGER counter;
    QueryPerformanceCounter(&counter);
    return (int64_t)((counter.QuadPart * 1000000000LL) / freq.QuadPart);
}

int64_t fast_time_epoch_ms(void) {
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    ULARGE_INTEGER uli;
    uli.LowPart = ft.dwLowDateTime;
    uli.HighPart = ft.dwHighDateTime;
    return (int64_t)((uli.QuadPart - 116444736000000000ULL) / 10000ULL);
}

void fast_time_sleep_ms(int64_t ms) {
    if (ms > 0) {
        Sleep((DWORD)ms);
    }
}

#else
#include <time.h>
#include <unistd.h>
#include <sys/time.h>

static void get_local_tm(struct tm* tm_out, int32_t* ms_out) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    localtime_r(&ts.tv_sec, tm_out);
    if (ms_out) *ms_out = (int32_t)(ts.tv_nsec / 1000000L);
}

int32_t fast_time_get_year(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)(t.tm_year + 1900);
}

int32_t fast_time_get_month(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)(t.tm_mon + 1);
}

int32_t fast_time_get_day(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)t.tm_mday;
}

int32_t fast_time_get_day_of_week(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)t.tm_wday;
}

int32_t fast_time_get_hour(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)t.tm_hour;
}

int32_t fast_time_get_minute(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)t.tm_min;
}

int32_t fast_time_get_second(void) {
    struct tm t;
    get_local_tm(&t, 0);
    return (int32_t)t.tm_sec;
}

int32_t fast_time_get_millisecond(void) {
    struct tm t;
    int32_t ms = 0;
    get_local_tm(&t, &ms);
    return ms;
}

int64_t fast_time_steady_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000LL + (int64_t)(ts.tv_nsec / 1000000LL);
}

int64_t fast_time_steady_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

int64_t fast_time_epoch_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (int64_t)ts.tv_sec * 1000LL + (int64_t)(ts.tv_nsec / 1000000LL);
}

void fast_time_sleep_ms(int64_t ms) {
    if (ms > 0) {
        usleep((useconds_t)(ms * 1000));
    }
}
#endif
