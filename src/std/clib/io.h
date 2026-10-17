#ifndef FASTLANG_CLIB_IO_H
#define FASTLANG_CLIB_IO_H

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <fcntl.h>

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
#include <io.h>
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

#ifndef FASTLANG_ALLOC_DEFINED
#define FASTLANG_ALLOC_DEFINED
static inline void* fast_alloc(size_t size)
{
    void* ptr = malloc(size);
    if (ptr) {
        memset(ptr, 0, size);
    }
    return ptr;
}

static inline void fast_free(void* ptr)
{
    if (ptr) {
        free(ptr);
    }
}
#endif

static inline int fast_open(const char* path, int flags, int mode)
{
#ifdef _WIN32
    if ((flags & 1) || (flags & 2)) {
        if (!(flags & 0x0100)) {
            flags |= 0x0100 | 0x0200; // _O_CREAT | _O_TRUNC
        }
        if (mode == 0) mode = 0x0100 | 0x0080; // _S_IREAD | _S_IWRITE
    }
    return _open(path, flags, mode);
#else
    if ((flags & 1) || (flags & 2)) {
        if (!(flags & 0100)) {
            flags |= 0100 | 01000; // O_CREAT | O_TRUNC
        }
        if (mode == 0) mode = 0666;
    }
    return open(path, flags, mode);
#endif
}

static inline int fast_close(int fd)
{
#ifdef _WIN32
    return _close(fd);
#else
    return close(fd);
#endif
}

static inline size_t fast_read(int fd, void* buf, size_t count)
{
    if (!buf || count == 0) return 0;
    if (fd == 0) {
        size_t read_bytes = fread(buf, 1, count, stdin);
        return read_bytes;
    }
#ifdef _WIN32
    int bytes = _read(fd, buf, (unsigned int)count);
    return bytes < 0 ? 0 : (size_t)bytes;
#else
    ssize_t bytes = read(fd, buf, count);
    return bytes < 0 ? 0 : (size_t)bytes;
#endif
}

static inline size_t fast_write(int fd, const void* buf, size_t count)
{
    fastlang_enable_utf8_output();
    if (!buf || count == 0) return 0;
    if (fd == 1) {
        size_t written = fwrite(buf, 1, count, stdout);
        fflush(stdout);
        return written;
    } else if (fd == 2) {
        size_t written = fwrite(buf, 1, count, stderr);
        fflush(stderr);
        return written;
    }
#ifdef _WIN32
    int bytes = _write(fd, buf, (unsigned int)count);
    return bytes < 0 ? 0 : (size_t)bytes;
#else
    ssize_t bytes = write(fd, buf, count);
    return bytes < 0 ? 0 : (size_t)bytes;
#endif
}

static inline char* fast_read_all(const char* path)
{
    if (!path) return NULL;
    FILE* f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (sz < 0) { fclose(f); return NULL; }
    size_t total_bytes = sizeof(size_t) + (size_t)sz + 1;
    void* mem = malloc(total_bytes);
    if (!mem) { fclose(f); return NULL; }
    size_t* raw = (size_t*)mem;
    char* buf = (char*)(raw + 1);
    size_t read_bytes = fread(buf, 1, sz, f);
    buf[read_bytes] = '\0';
    raw[0] = read_bytes;
    fclose(f);
    return buf;
}

#ifdef __cplusplus
static inline char32_t* fast_read_all_u(const char* path)
#else
static inline uint32_t* fast_read_all_u(const char* path)
#endif
{
    char* utf8 = fast_read_all(path);
    if (!utf8) return NULL;
    size_t utf8_len = ((size_t*)utf8)[-1];
    size_t cp_count = 0;
    for (size_t i = 0; i < utf8_len; ) {
        unsigned char c = (unsigned char)utf8[i];
        if (c < 0x80) i += 1;
        else if ((c & 0xE0) == 0xC0) i += 2;
        else if ((c & 0xF0) == 0xE0) i += 3;
        else if ((c & 0xF8) == 0xF0) i += 4;
        else i += 1;
        cp_count++;
    }
    void* mem = malloc(sizeof(size_t) + (cp_count + 1) * sizeof(uint32_t));
    if (!mem) {
        free(((size_t*)utf8) - 1);
        return NULL;
    }
    size_t* raw = (size_t*)mem;
#ifdef __cplusplus
    char32_t* buf = (char32_t*)(raw + 1);
#else
    uint32_t* buf = (uint32_t*)(raw + 1);
#endif
    raw[0] = cp_count;

    size_t out_idx = 0;
    for (size_t i = 0; i < utf8_len; ) {
        unsigned char c = (unsigned char)utf8[i];
        uint32_t cp = 0;
        if (c < 0x80) {
            cp = c;
            i += 1;
        } else if ((c & 0xE0) == 0xC0 && i + 1 < utf8_len) {
            cp = ((c & 0x1F) << 6) | (utf8[i + 1] & 0x3F);
            i += 2;
        } else if ((c & 0xF0) == 0xE0 && i + 2 < utf8_len) {
            cp = ((c & 0x0F) << 12) | ((utf8[i + 1] & 0x3F) << 6) | (utf8[i + 2] & 0x3F);
            i += 3;
        } else if ((c & 0xF8) == 0xF0 && i + 3 < utf8_len) {
            cp = ((c & 0x07) << 18) | ((utf8[i + 1] & 0x3F) << 12) | ((utf8[i + 2] & 0x3F) << 6) | (utf8[i + 3] & 0x3F);
            i += 4;
        } else {
            cp = c;
            i += 1;
        }
        buf[out_idx++] = cp;
    }
    buf[cp_count] = 0;
    free(((size_t*)utf8) - 1);
    return buf;
}

static inline char* fast_uchar_to_utf8(const void* u_ptr)
{
    if (!u_ptr) return NULL;
    const uint32_t* u = (const uint32_t*)u_ptr;
    size_t u_len = ((size_t*)u)[-1];
    size_t byte_count = 0;
    for (size_t i = 0; i < u_len; i++) {
        uint32_t val = u[i];
        if (val <= 0x7F) byte_count += 1;
        else if (val <= 0x7FF) byte_count += 2;
        else if (val <= 0xFFFF) byte_count += 3;
        else byte_count += 4;
    }
    void* mem = malloc(sizeof(size_t) + byte_count + 1);
    if (!mem) return NULL;
    size_t* raw = (size_t*)mem;
    char* buf = (char*)(raw + 1);
    raw[0] = byte_count;

    size_t out_idx = 0;
    for (size_t i = 0; i < u_len; i++) {
        uint32_t val = u[i];
        if (val <= 0x7F) {
            buf[out_idx++] = (char)val;
        } else if (val <= 0x7FF) {
            buf[out_idx++] = (char)(0xC0 | ((val >> 6) & 0x1F));
            buf[out_idx++] = (char)(0x80 | (val & 0x3F));
        } else if (val <= 0xFFFF) {
            buf[out_idx++] = (char)(0xE0 | ((val >> 12) & 0x0F));
            buf[out_idx++] = (char)(0x80 | ((val >> 6) & 0x3F));
            buf[out_idx++] = (char)(0x80 | (val & 0x3F));
        } else {
            buf[out_idx++] = (char)(0xF0 | ((val >> 18) & 0x07));
            buf[out_idx++] = (char)(0x80 | ((val >> 12) & 0x3F));
            buf[out_idx++] = (char)(0x80 | ((val >> 6) & 0x3F));
            buf[out_idx++] = (char)(0x80 | (val & 0x3F));
        }
    }
    buf[byte_count] = '\0';
    return buf;
}

static inline void fast_exit(int code)
{
    exit(code);
}

#endif // FASTLANG_CLIB_IO_H
