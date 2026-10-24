#ifndef FASTLANG_CLIB_MATH_H
#define FASTLANG_CLIB_MATH_H

#ifdef __cplusplus
extern "C" {
#endif

double fast_sqrt(double x);
double fast_pow(double base, double exp);
double fast_sin(double x);
double fast_cos(double x);
double fast_tan(double x);
double fast_floor(double x);
double fast_ceil(double x);

#ifdef __cplusplus
}
#endif

#endif

