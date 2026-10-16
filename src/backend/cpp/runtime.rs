pub fn generate_runtime_header() -> String {
    r#"#pragma once
#include <variant>
#include <cstdint>
#include <type_traits>
#include <cstring>
#include <vector>
#include <memory>

template <typename Ret, typename... Args>
struct fastlang_fn_ptr {
    void* ctx = nullptr;
    Ret (*invoke)(void* ctx, Args...) = nullptr;

    constexpr fastlang_fn_ptr() = default;
    constexpr fastlang_fn_ptr(std::nullptr_t) : ctx(nullptr), invoke(nullptr) {}

    constexpr fastlang_fn_ptr(Ret (*fn)(Args...))
        : ctx(reinterpret_cast<void*>(fn)),
          invoke([](void* c, Args... args) -> Ret {
              auto f = reinterpret_cast<Ret (*)(Args...)>(c);
              if constexpr (std::is_void_v<Ret>) {
                  f(static_cast<Args&&>(args)...);
              } else {
                  return f(static_cast<Args&&>(args)...);
              }
          }) {}

    template <typename F, typename = std::enable_if_t<!std::is_same_v<std::decay_t<F>, fastlang_fn_ptr>>>
    fastlang_fn_ptr(F&& f)
        : ctx(reinterpret_cast<void*>(&f)),
          invoke([](void* c, Args... args) -> Ret {
              auto& fn = *reinterpret_cast<std::decay_t<F>*>(c);
              if constexpr (std::is_void_v<Ret>) {
                  fn(static_cast<Args&&>(args)...);
              } else {
                  return fn(static_cast<Args&&>(args)...);
              }
          }) {}

    inline Ret operator()(Args... args) const {
        return invoke(ctx, static_cast<Args&&>(args)...);
    }
    inline explicit operator bool() const {
        return invoke != nullptr;
    }
};

using type = const char*;
// ============================================================================
// === [FastLang Array Runtime Implementation] ================================
// === (Temporary C++ Interop - To be migrated to Native Alloc in next release)
// ============================================================================

template <typename T>
inline T* fastlang_array_alloc(size_t len) {
    size_t total_bytes = sizeof(size_t) + (len + 1) * sizeof(T);
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    std::memset(mem, 0, total_bytes);
    size_t* raw = reinterpret_cast<size_t*>(mem);
    raw[0] = len;
    T* data = reinterpret_cast<T*>(raw + 1);
    for (size_t i = 0; i < len; ++i) {
        new (&data[i]) T();
    }
    data[len] = T();
    return data;
}

struct fastlang_empty_array_t {
    template <typename T>
    operator T*() const {
        return fastlang_array_alloc<T>(0);
    }
    template <typename T, typename = std::enable_if_t<!std::is_pointer_v<T>>>
    operator T() const {
        return T{};
    }
};

template <typename T>
inline auto fastlang_array_len(const T& arr) {
    using Decayed = std::decay_t<T>;
    if constexpr (std::is_same_v<Decayed, fastlang_empty_array_t>) {
        return (size_t)0;
    } else if constexpr (std::is_pointer_v<Decayed>) {
        if (!arr) return (size_t)0;
        return reinterpret_cast<const size_t*>(arr)[-1];
    } else {
        return arr.len;
    }
}

template <typename T>
inline T* fastlang_array_create(std::initializer_list<T> list) {
    size_t len = list.size();
    size_t total_bytes = sizeof(size_t) + (len + 1) * sizeof(T);
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    std::memset(mem, 0, total_bytes);
    size_t* raw = reinterpret_cast<size_t*>(mem);
    raw[0] = len;
    T* data = reinterpret_cast<T*>(raw + 1);
    size_t i = 0;
    for (const auto& item : list) {
        new (&data[i++]) T(item);
    }
    data[len] = T();
    return data;
}

inline char* fastlang_string_create(std::initializer_list<char> list) {
    size_t len = list.size();
    const char* first = list.begin();
    if (len > 0 && *(first + len - 1) == '\0') {
        len -= 1;
    }
    size_t total_bytes = sizeof(size_t) + len + 1;
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    size_t* raw = reinterpret_cast<size_t*>(mem);
    raw[0] = len;
    char* data = reinterpret_cast<char*>(raw + 1);
    size_t i = 0;
    for (char c : list) {
        if (i < len) data[i++] = c;
    }
    data[len] = '\0';
    return data;
}

inline char* fastlang_string_create_from_ptr(const char* s) {
    if (!s) s = "";
    size_t len = std::strlen(s);
    size_t total_bytes = sizeof(size_t) + len + 1;
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    size_t* raw = reinterpret_cast<size_t*>(mem);
    raw[0] = len;
    char* data = reinterpret_cast<char*>(raw + 1);
    std::memcpy(data, s, len + 1);
    return data;
}

inline char32_t* fastlang_ustring_create_from_ptr(const char32_t* s) {
    if (!s) s = U"";
    size_t len = 0;
    while (s[len] != 0) ++len;
    size_t total_bytes = sizeof(size_t) + (len + 1) * sizeof(char32_t);
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    size_t* raw = reinterpret_cast<size_t*>(mem);
    raw[0] = len;
    char32_t* data = reinterpret_cast<char32_t*>(raw + 1);
    for (size_t i = 0; i < len; ++i) data[i] = s[i];
    data[len] = 0;
    return data;
}

struct fastlang_strbuf {
    char* data = nullptr;
    size_t capacity = 0;
    size_t length = 0;

    void append(const char* s) {
        if (!s) return;
        size_t slen = std::strlen(s);
        if (length + slen + 1 > capacity) {
            size_t new_cap = (capacity == 0) ? 256 : (capacity * 2);
            while (new_cap < length + slen + 1) new_cap *= 2;
            char* next = (char*)std::realloc(data, new_cap);
            if (!next) return;
            data = next;
            capacity = new_cap;
        }
        std::memcpy(data + length, s, slen);
        length += slen;
        data[length] = '\0';
    }

    char* finalize() {
        if (!data) return fastlang_string_create_from_ptr("");
        size_t total_bytes = sizeof(size_t) + length + 1;
        void* mem = std::malloc(total_bytes);
        if (!mem) { std::free(data); return nullptr; }
        size_t* raw = reinterpret_cast<size_t*>(mem);
        raw[0] = length;
        char* out = reinterpret_cast<char*>(raw + 1);
        std::memcpy(out, data, length + 1);
        std::free(data);
        return out;
    }
};

template <typename T>
inline void fastlang_array_free(T* arr) {
    if (!arr) return;
    size_t len = fastlang_array_len(arr);
    for (size_t i = 0; i < len; ++i) {
        arr[i].~T();
    }
    size_t* raw = reinterpret_cast<size_t*>(arr) - 1;
    std::free(raw);
}

namespace fast_std {
template <typename T>
using array = T*;
using str = char*;
using ustr = char32_t*;
}

using fast_std::array;
using fast_std::str;
using fast_std::ustr;
// ============================================================================

struct fastlang_unit_t {
    bool operator==(const fastlang_unit_t&) const { return true; }
    bool operator!=(const fastlang_unit_t&) const { return false; }
};

struct fastlang_undefined_t {
    template <typename T>
    operator T() const { return T{}; }
    bool operator==(const fastlang_undefined_t&) const { return true; }
    bool operator!=(const fastlang_undefined_t&) const { return false; }
};

template <typename T, typename = void>
struct has_copy : std::false_type {};
template <typename T>
struct has_copy<T, std::void_t<decltype(std::declval<T&>().copy())>> : std::true_type {};

template <typename T>
inline auto fastlang_copy(const T& val) {
    if constexpr (has_copy<T>::value) {
        return const_cast<T&>(val).copy();
    } else {
        return val;
    }
}

namespace fast_std {
template <typename T>
bool fastlang_array_partial_equal(T* __this, T* other);
template <typename T>
bool fastlang_array_not_equal(T* __this, T* other);

inline fast_std::array<char> fastlang_array_add(fast_std::array<char> __this, fast_std::array<char> other);
inline fast_std::array<char> fastlang_array_add(fast_std::array<char> __this, char other);
template <typename Other>
inline fast_std::array<char> fastlang_array_add(fast_std::array<char> __this, Other other);
template <typename T>
inline fast_std::array<T> fastlang_array_add(fast_std::array<T> __this, fast_std::array<T> other);
template <typename T>
inline fast_std::array<T> fastlang_array_add(fast_std::array<T> __this, T item);
inline fast_std::array<char> fastlang_char_add(char __this, fast_std::array<char> other);
}

// Forward declarations for string conversion
template <typename T>
inline char* fastlang_as_str(const T& val);
inline char* fastlang_as_str(bool b);
inline char* fastlang_as_str(char* s);
inline char* fastlang_as_str(const char* s);
inline char* fastlang_as_str(char32_t* s);
inline char* fastlang_as_str(const char32_t* s);
template <typename T>
inline char32_t* fastlang_as_ustr(const T& val);
inline char32_t* fastlang_as_ustr(char32_t* s);
inline char32_t* fastlang_as_ustr(const char32_t* s);
inline char32_t* fastlang_as_ustr(char* s);
inline char32_t* fastlang_as_ustr(const char* s);


template <typename A, typename B>
inline bool fastlang_eq(A&& a, B&& b) {
    using DecA = std::decay_t<A>;
    using DecB = std::decay_t<B>;
    if constexpr ((std::is_same_v<DecA, char*> || std::is_same_v<DecA, const char*>) && (std::is_same_v<DecB, char*> || std::is_same_v<DecB, const char*>)) {
        if (a == b) return true;
        if (!a || !b) return false;
        return fast_std::fastlang_array_partial_equal(const_cast<char*>(a), const_cast<char*>(b));
    } else {
        return a == b;
    }
}

template <typename A, typename B>
inline bool fastlang_ne(A&& a, B&& b) {
    using DecA = std::decay_t<A>;
    using DecB = std::decay_t<B>;
    if constexpr ((std::is_same_v<DecA, char*> || std::is_same_v<DecA, const char*>) && (std::is_same_v<DecB, char*> || std::is_same_v<DecB, const char*>)) {
        if (a == b) return false;
        if (!a || !b) return true;
        return fast_std::fastlang_array_not_equal(const_cast<char*>(a), const_cast<char*>(b));
    } else {
        return a != b;
    }
}

template <typename T, typename = void>
struct fastlang_has_as_str : std::false_type {};
template <typename T>
struct fastlang_has_as_str<T, std::void_t<decltype(std::declval<T>().as_str())>> : std::true_type {};

template <typename T, typename = void>
struct has_display : std::false_type {};
template <typename T>
struct has_display<T, std::void_t<decltype(std::declval<T>().display())>> : std::true_type {};

inline char* fastlang_as_str(bool b) { return fastlang_string_create_from_ptr(b ? "true" : "false"); }
inline char* fastlang_as_str(char* s) { return s ? s : fastlang_string_create_from_ptr(""); }
inline char* fastlang_as_str(const char* s) { return fastlang_string_create_from_ptr(s ? s : ""); }

inline char* fastlang_uchar_to_utf8(const char32_t* u_str) {
    if (!u_str) return fastlang_string_create_from_ptr("");
    size_t u_len = fastlang_array_len(u_str);
    size_t actual_len = 0;
    while (actual_len < u_len && u_str[actual_len] != 0) {
        actual_len++;
    }
    size_t byte_count = 0;
    for (size_t i = 0; i < actual_len; ++i) {
        uint32_t val = (uint32_t)u_str[i];
        if (val <= 0x7F) byte_count += 1;
        else if (val <= 0x7FF) byte_count += 2;
        else if (val <= 0xFFFF) byte_count += 3;
        else byte_count += 4;
    }
    size_t total_bytes = sizeof(size_t) + byte_count + 1;
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    size_t* raw = reinterpret_cast<size_t*>(mem);
    raw[0] = byte_count;
    char* buf = reinterpret_cast<char*>(raw + 1);
    size_t out_idx = 0;
    for (size_t i = 0; i < actual_len; ++i) {
        uint32_t val = (uint32_t)u_str[i];
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

inline char* fastlang_as_str(char32_t* s) { return fastlang_uchar_to_utf8(s); }
inline char* fastlang_as_str(const char32_t* s) { return fastlang_uchar_to_utf8(s); }

inline char32_t* fastlang_utf8_to_uchar(const char* s) {
    if (!s) return fastlang_ustring_create_from_ptr(U"");
    size_t s_len = std::strlen(s);
    size_t total_bytes = sizeof(size_t) + (s_len + 1) * sizeof(char32_t);
    void* mem = std::malloc(total_bytes);
    if (!mem) return nullptr;
    size_t* raw = reinterpret_cast<size_t*>(mem);
    char32_t* buf = reinterpret_cast<char32_t*>(raw + 1);
    size_t out_idx = 0;
    size_t i = 0;
    while (i < s_len) {
        unsigned char c = (unsigned char)s[i];
        if (c < 0x80) {
            buf[out_idx++] = (char32_t)c;
            i += 1;
        } else if ((c >> 5) == 0x06) {
            uint32_t code = (c & 0x1F) << 6;
            if (i + 1 < s_len) code |= ((unsigned char)s[i + 1] & 0x3F);
            buf[out_idx++] = (char32_t)code;
            i += 2;
        } else if ((c >> 4) == 0x0E) {
            uint32_t code = (c & 0x0F) << 12;
            if (i + 1 < s_len) code |= (((unsigned char)s[i + 1] & 0x3F) << 6);
            if (i + 2 < s_len) code |= ((unsigned char)s[i + 2] & 0x3F);
            buf[out_idx++] = (char32_t)code;
            i += 3;
        } else if ((c >> 3) == 0x1E) {
            uint32_t code = (c & 0x07) << 18;
            if (i + 1 < s_len) code |= (((unsigned char)s[i + 1] & 0x3F) << 12);
            if (i + 2 < s_len) code |= (((unsigned char)s[i + 2] & 0x3F) << 6);
            if (i + 3 < s_len) code |= ((unsigned char)s[i + 3] & 0x3F);
            buf[out_idx++] = (char32_t)code;
            i += 4;
        } else {
            buf[out_idx++] = (char32_t)c;
            i += 1;
        }
    }
    buf[out_idx] = 0;
    raw[0] = out_idx;
    return buf;
}

inline char32_t* fastlang_as_ustr(char32_t* s) { return s ? s : fastlang_ustring_create_from_ptr(U""); }
inline char32_t* fastlang_as_ustr(const char32_t* s) { return fastlang_ustring_create_from_ptr(s ? s : U""); }
inline char32_t* fastlang_as_ustr(char* s) { return fastlang_utf8_to_uchar(s); }
inline char32_t* fastlang_as_ustr(const char* s) { return fastlang_utf8_to_uchar(s); }

template <typename T>
inline char32_t* fastlang_as_ustr(const T& val) {
    using Decayed = std::decay_t<T>;
    if constexpr (std::is_same_v<Decayed, const char32_t*> || std::is_same_v<Decayed, char32_t*>) {
        return fastlang_as_ustr(val);
    } else if constexpr (std::is_same_v<Decayed, const char*> || std::is_same_v<Decayed, char*>) {
        return fastlang_utf8_to_uchar(val);
    } else if constexpr (std::is_same_v<Decayed, char32_t>) {
        char32_t single[2] = {val, 0};
        return fastlang_ustring_create_from_ptr(single);
    } else if constexpr (std::is_same_v<Decayed, char>) {
        char buf[2] = {val, '\0'};
        return fastlang_utf8_to_uchar(buf);
    } else {
        char* s = fastlang_as_str(val);
        return fastlang_utf8_to_uchar(s);
    }
}

template <typename T>
inline char* fastlang_as_str(const T& val) {
    using Decayed = std::decay_t<T>;
    if constexpr (std::is_same_v<Decayed, const char*> || std::is_same_v<Decayed, char*>) {
        return fastlang_string_create_from_ptr(val ? val : "");
    } else if constexpr (std::is_same_v<Decayed, char>) {
        char buf[2] = {val, '\0'};
        return fastlang_string_create_from_ptr(buf);
    } else if constexpr (std::is_same_v<Decayed, bool>) {
        return fastlang_string_create_from_ptr(val ? "true" : "false");
    } else if constexpr (std::is_same_v<Decayed, fastlang_unit_t>) {
        return fastlang_string_create_from_ptr("()");
    } else if constexpr (std::is_same_v<Decayed, fastlang_undefined_t>) {
        return fastlang_string_create_from_ptr("undefined");
    } else if constexpr (std::is_signed_v<Decayed> && (std::is_same_v<Decayed, int8_t> || std::is_same_v<Decayed, int16_t> || std::is_same_v<Decayed, int32_t> || std::is_same_v<Decayed, int64_t>)) {
        char buf[64];
        snprintf(buf, sizeof(buf), "%lld", (long long)val);
        return fastlang_string_create_from_ptr(buf);
    } else if constexpr (std::is_unsigned_v<Decayed> && (std::is_same_v<Decayed, uint8_t> || std::is_same_v<Decayed, uint16_t> || std::is_same_v<Decayed, uint32_t> || std::is_same_v<Decayed, uint64_t>)) {
        char buf[64];
        snprintf(buf, sizeof(buf), "%llu", (unsigned long long)val);
        return fastlang_string_create_from_ptr(buf);
    } else if constexpr (std::is_floating_point_v<Decayed>) {
        char buf[64];
        snprintf(buf, sizeof(buf), "%g", (double)val);
        return fastlang_string_create_from_ptr(buf);
    } else if constexpr (has_display<Decayed>::value) {
        return fastlang_string_create_from_ptr(const_cast<Decayed&>(val).display());
    } else if constexpr (fastlang_has_as_str<Decayed>::value) {
        return fastlang_string_create_from_ptr(const_cast<Decayed&>(val).as_str());
    } else if constexpr (std::is_same_v<Decayed, char32_t>) {
        char32_t single[2] = {val, 0};
        return fastlang_uchar_to_utf8(single);
    } else if constexpr (std::is_same_v<Decayed, const char32_t*> || std::is_same_v<Decayed, char32_t*>) {
        return fastlang_uchar_to_utf8(val);
    } else if constexpr (std::is_pointer_v<Decayed>) {
        if (!val) return fastlang_string_create_from_ptr("None");
        return fastlang_as_str(*val);
    } else {
        return fastlang_string_create_from_ptr("[object]");
    }
}

template <typename T>
inline char* fastlang_as_str(T* ptr) {
    if (!ptr) return fastlang_string_create_from_ptr("None");
    if constexpr (std::is_same_v<T, char32_t> || std::is_same_v<T, const char32_t>) {
        return fastlang_uchar_to_utf8(ptr);
    } else {
        return fastlang_as_str(*ptr);
    }
}

template <typename T>
inline char* fastlang_to_str(const T& val) {
    return fastlang_as_str(val);
}

template <typename Target, typename Candidate>
bool fastlang_match_eq(const Target& target, const Candidate& candidate) {
    if constexpr (std::is_invocable_r_v<Target, Candidate>) {
        return target == candidate();
    } else {
        return target == candidate;
    }
}

namespace fast_std {
    template <typename T> struct iterator;
    template <typename T> inline iterator<T> fastlang_array_iter(fast_std::array<T> arr);
}

template <typename T, typename = void> struct has_iter_fn : std::false_type {};
template <typename T> struct has_iter_fn<T, std::void_t<decltype(std::declval<T>().iter())>> : std::true_type {};

template <typename T, typename = void> struct has_iterator_fn : std::false_type {};
template <typename T> struct has_iterator_fn<T, std::void_t<decltype(std::declval<T>().iterator())>> : std::true_type {};

template <typename T, typename = void> struct has_ptr_iter_fn : std::false_type {};
template <typename T> struct has_ptr_iter_fn<T, std::void_t<decltype(std::declval<T>()->iter())>> : std::true_type {};

template <typename T>
inline auto fastlang_get_iter(T&& col) {
    using Decayed = std::decay_t<T>;
    if constexpr (std::is_pointer_v<Decayed>) {
        return fast_std::fastlang_array_iter(col);
    } else if constexpr (has_iter_fn<Decayed>::value) {
        return col.iter();
    } else if constexpr (has_iterator_fn<Decayed>::value) {
        return col.iterator();
    } else if constexpr (has_ptr_iter_fn<Decayed>::value) {
        return col->iter();
    } else {
        return col;
    }
}
template <typename T, size_t N>
inline auto fastlang_get_iter(T (&arr)[N]) {
    return fast_std::fastlang_array_iter(static_cast<T*>(arr));
}

template <typename T = void>
struct fastlang_spread_acc {
    std::vector<T> items;
    template <typename U>
    void push_one(const U& x) { items.push_back(static_cast<T>(x)); }
    template <typename C>
    void push_spread(const C& c) {
        if constexpr (std::is_pointer_v<std::decay_t<C>>) {
            if (c) {
                size_t len = fastlang_array_len(c);
                for (size_t i = 0; i < len; ++i) {
                    items.push_back(static_cast<T>(c[i]));
                }
            }
        } else {
            for (auto&& x : c) items.push_back(static_cast<T>(x));
        }
    }
    fast_std::array<T> to_array() {
        if (items.empty()) return fastlang_array_alloc<T>(0);
        T* arr = fastlang_array_alloc<T>(items.size());
        for (size_t i = 0; i < items.size(); ++i) arr[i] = items[i];
        return arr;
    }
    fast_std::array<T> to_slice() {
        return to_array();
    }
};

template<typename T, typename = void>
struct fastlang_type_default_helper {
    static T get() { return T{}; }
};
template<typename T>
struct fastlang_type_default_helper<T, std::void_t<decltype(T::fastlang_handle_default())>> {
    static T get() { return T::fastlang_handle_default(); }
};
template<typename T>
inline T fastlang_type_default() {
    return fastlang_type_default_helper<T>::get();
}
"#.to_string()
}
