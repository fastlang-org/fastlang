#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    TEST_STATUS_OFF = 0,
    TEST_STATUS_ON = 1,
    TEST_STATUS_UNKNOWN = 2
} TestStatus;

typedef struct {
    int32_t id;
    int32_t val;
    double scale;
} TestDevice;

typedef int32_t CustomDevId;

static inline TestDevice make_device(int32_t id, int32_t val, double scale) {
    TestDevice d;
    d.id = id;
    d.val = val;
    d.scale = scale;
    return d;
}

static inline int32_t compute_device_score(TestDevice dev) {
    return (int32_t)(dev.val * dev.scale) + dev.id;
}

#ifdef __cplusplus
}
#endif
