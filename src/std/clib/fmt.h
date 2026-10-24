#ifndef FASTLANG_CLIB_FMT_H
#define FASTLANG_CLIB_FMT_H

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOGDI
#define NOGDI
#endif
#ifndef NOUSER
#define NOUSER
#endif
#include <winsock2.h>
#include <windows.h>
#undef NOGDI
#undef NOUSER
#undef CloseWindow
#undef ShowCursor
#undef LoadImage
#undef DrawText
#undef DrawTextEx
#else
#include <unistd.h>
#endif

#ifndef FASTLANG_UTF8_DEFINED
#define FASTLANG_UTF8_DEFINED
static inline void fastlang_enable_utf8_output(void)
{
#ifdef _WIN32
    static int initialized = 0;
    if (!initialized)
    {
        SetConsoleOutputCP(CP_UTF8);
        initialized = 1;
    }
#endif
}
#endif

#ifdef __cplusplus
#if __has_include(<clib/fast_runtime.h>)
#include <clib/fast_runtime.h>
#elif __has_include("fast_runtime.h")
#include "fast_runtime.h"
#endif
#include <type_traits>

#ifndef FASTLANG_HAS_DISPLAY_DEFINED
#define FASTLANG_HAS_DISPLAY_DEFINED
template <typename T, typename = void>
struct has_display : std::false_type {};

template <typename T>
struct has_display<T, std::void_t<decltype(std::declval<T>().display())>> : std::true_type {};
#endif

template <typename T>
static inline void print_single_arg(const T &val)
{
    fastlang_enable_utf8_output();
    using Decayed = std::decay_t<T>;
    if constexpr (std::is_same_v<Decayed, const char *> || std::is_same_v<Decayed, char *>)
    {
        if (val)
            fputs(val, stdout);
    }
    else if constexpr (std::is_same_v<Decayed, const char32_t *> || std::is_same_v<Decayed, char32_t *>)
    {
        if (val)
        {
            size_t len = fastlang_array_len(val);
            for (size_t i = 0; i < len; ++i)
            {
                if (val[i] == 0)
                    break;
                print_single_arg(val[i]);
            }
        }
    }
    else if constexpr (std::is_same_v<Decayed, char>)
    {
        putchar(val);
    }
    else if constexpr (std::is_same_v<Decayed, char32_t>)
    {
        if (val <= 0x7F)
        {
            putchar((int)val);
        }
        else if (val <= 0x7FF)
        {
            putchar((int)(0xC0 | ((val >> 6) & 0x1F)));
            putchar((int)(0x80 | (val & 0x3F)));
        }
        else if (val <= 0xFFFF)
        {
            putchar((int)(0xE0 | ((val >> 12) & 0x0F)));
            putchar((int)(0x80 | ((val >> 6) & 0x3F)));
            putchar((int)(0x80 | (val & 0x3F)));
        }
        else if (val <= 0x10FFFF)
        {
            putchar((int)(0xF0 | ((val >> 18) & 0x07)));
            putchar((int)(0x80 | ((val >> 12) & 0x3F)));
            putchar((int)(0x80 | ((val >> 6) & 0x3F)));
            putchar((int)(0x80 | (val & 0x3F)));
        }
    }
    else if constexpr (std::is_same_v<Decayed, bool>)
    {
        fputs(val ? "true" : "false", stdout);
    }
    else if constexpr (std::is_same_v<Decayed, fastlang_unit_t>)
    {
        fputs("()", stdout);
    }
    else if constexpr (std::is_same_v<Decayed, fastlang_undefined_t>)
    {
        fputs("undefined", stdout);
    }
    else if constexpr (std::is_same_v<Decayed, uint8_t> || std::is_same_v<Decayed, int8_t>)
    {
        if constexpr (std::is_signed_v<Decayed>)
        {
            printf("%d", (int)val);
        }
        else
        {
            printf("%u", (unsigned int)val);
        }
    }
    else if constexpr (std::is_integral_v<Decayed>)
    {
        if constexpr (std::is_signed_v<Decayed>)
        {
            printf("%lld", (long long)val);
        }
        else
        {
            printf("%llu", (unsigned long long)val);
        }
    }
    else if constexpr (std::is_floating_point_v<Decayed>)
    {
        printf("%g", (double)val);
    }
    else if constexpr (has_display<Decayed>::value)
    {
        auto d = const_cast<Decayed &>(val).display();
        print_single_arg(d);
    }
    else if constexpr (std::is_pointer_v<Decayed>)
    {
        if (!val)
        {
            fputs("None", stdout);
        }
        else
        {
            size_t len = fastlang_array_len(val);
            putchar('[');
            for (size_t i = 0; i < len; ++i)
            {
                if (i > 0)
                    fputs(", ", stdout);
                print_single_arg(val[i]);
            }
            putchar(']');
        }
    }
    else
    {
        auto s = fastlang_as_str(val);
        if (s)
        {
            fputs(s, stdout);
        }
    }
}

template <typename... Args>
static inline void fast_print(const Args &...args)
{
    fastlang_enable_utf8_output();
    ((print_single_arg(args)), ...);
}

template <typename... Args>
static inline void fast_println(const Args &...args)
{
    fastlang_enable_utf8_output();
    ((print_single_arg(args)), ...);
    putchar('\n');
}

static inline void fast_print_str(const char *s)
{
    fastlang_enable_utf8_output();
    if (s)
        fputs(s, stdout);
}
template <typename T>
static inline void fast_print_str(const T &s)
{
    fastlang_enable_utf8_output();
    print_single_arg(s);
}
static inline void fast_print_int(long long val)
{
    fastlang_enable_utf8_output();
    printf("%lld", val);
}
static inline void fast_print_double(double val)
{
    fastlang_enable_utf8_output();
    printf("%g", val);
}
static inline void fast_print_char(int c)
{
    fastlang_enable_utf8_output();
    putchar(c);
}
static inline void fast_print_bool(int b)
{
    fastlang_enable_utf8_output();
    fputs(b ? "true" : "false", stdout);
}

static inline char *fast_input(const char *prompt = "")
{
    fastlang_enable_utf8_output();
    if (prompt && *prompt)
    {
        fputs(prompt, stdout);
        fflush(stdout);
    }
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin))
    {
        size_t len = strlen(buf);
        if (len > 0 && buf[len - 1] == '\n')
        {
            buf[len - 1] = '\0';
            len--;
        }
        if (len > 0 && buf[len - 1] == '\r')
        {
            buf[len - 1] = '\0';
            len--;
        }
        return fastlang_string_create_from_ptr(buf);
    }
    return fastlang_string_create_from_ptr("");
}

#else

static inline void fast_println(const char *s)
{
    fastlang_enable_utf8_output();
    if (s)
        puts(s);
    else
        putchar('\n');
}
static inline void fast_print_int(long long val) { printf("%lld\n", val); }
static inline void fast_print_double(double val) { printf("%g\n", val); }
static inline void fast_print_char(int c)
{
    putchar(c);
    putchar('\n');
}
static inline void fast_print_bool(int b) { puts(b ? "true" : "false"); }
static inline void fast_print_str(const char *s)
{
    if (s)
        fputs(s, stdout);
}

static inline char *fast_input(const char *prompt)
{
    if (prompt && *prompt)
    {
        fputs(prompt, stdout);
        fflush(stdout);
    }
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin))
    {
        size_t len = strlen(buf);
        if (len > 0 && buf[len - 1] == '\n')
        {
            buf[len - 1] = '\0';
            len--;
        }
        if (len > 0 && buf[len - 1] == '\r')
        {
            buf[len - 1] = '\0';
            len--;
        }
        char *res = (char *)malloc(len + 1);
        if (res)
            memcpy(res, buf, len + 1);
        return res;
    }
    char *empty = (char *)malloc(1);
    if (empty)
        empty[0] = '\0';
    return empty;
}

#endif

#endif // FASTLANG_CLIB_FMT_H
