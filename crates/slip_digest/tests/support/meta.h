// The `std::meta` names the digest's reflection pair tests use (WP-038;
// spec/harness.md §6.1): a test writes `#include "meta.h"`. A subset of
// Kiln's own test header (kiln/tests/support/kiln_meta.h), declared as
// it declares them: the functions with no body are intrinsics Kiln's
// Oven dispatches by qualified name and signature, and the class types
// they read or return have the layout it matches (spec/reflection.md
// §3.5 in Kiln). Nothing here is a template whose return type is
// deduced. The four constructors defined in a template (initializer_list's
// and optional's default constructors, vector's move constructor and
// optional's converting one) are indeterminate,
// kiln/pattern-mem-initializers, until Kiln's WP-3026 puts their
// mem-initializers on the AST (spec/landed/digest/WP-124.md): every
// indeterminate test that includes this file lists them.
#pragma once

namespace std {
using __kiln_size_t = decltype(sizeof 0);

template <class E>
class initializer_list {
  const E* __begin_;
  __kiln_size_t __size_;

 public:
  constexpr initializer_list() noexcept : __begin_(nullptr), __size_(0) {}
  constexpr __kiln_size_t size() const noexcept { return __size_; }
  constexpr const E* begin() const noexcept { return __begin_; }
  constexpr const E* end() const noexcept { return __begin_ + __size_; }
};

template <class T>
struct allocator {
  constexpr allocator() noexcept = default;
  constexpr T* allocate(__kiln_size_t n) { return static_cast<T*>(::operator new(n * sizeof(T))); }
  constexpr void deallocate(T* p, __kiln_size_t) noexcept { ::operator delete(p); }
};

template <class T, class A = allocator<T>>
class vector {
  T* __begin_ = nullptr;
  T* __end_ = nullptr;
  T* __cap_ = nullptr;

 public:
  constexpr vector() noexcept = default;
  vector(const vector&) = delete;
  constexpr vector(vector&& o) noexcept : __begin_(o.__begin_), __end_(o.__end_), __cap_(o.__cap_) {
    o.__begin_ = o.__end_ = o.__cap_ = nullptr;
  }
  vector& operator=(const vector&) = delete;
  constexpr ~vector() {
    if (__begin_) A().deallocate(__begin_, __kiln_size_t(__cap_ - __begin_));
  }
  constexpr const T& operator[](__kiln_size_t i) const noexcept { return __begin_[i]; }
};

template <class T>
class optional {
  union {
    char __null_state_;
    T __val_;
  };
  bool __engaged_ = false;

 public:
  constexpr optional() noexcept : __null_state_() {}
  template <class U = T>
    requires(!__is_same(__remove_cvref(U), optional))
  constexpr optional(U&& v) : __val_(static_cast<U&&>(v)), __engaged_(true) {}
};
}  // namespace std

namespace std::meta {
using info = decltype(^^int);

class access_context {
  consteval access_context(info s, info d) noexcept : __scope_(s), __designating_(d) {}

 public:
  access_context() = delete;
  static consteval access_context unchecked() noexcept { return {info(), info()}; }

 private:
  info __scope_;
  info __designating_;
};

consteval std::vector<info> bases_of(info r, access_context ctx);
consteval std::vector<info> parameters_of(info r);
consteval std::vector<info> annotations_of(info item);

struct data_member_options {
  struct name_type {
    bool __u8_ = false;
    const char8_t* __u8p_ = nullptr;
    decltype(sizeof 0) __u8n_ = 0;
    const char* __p_ = nullptr;
    decltype(sizeof 0) __n_ = 0;

    consteval name_type(const char* s) : __p_(s), __n_(__builtin_strlen(s)) {}
  };
  std::optional<name_type> name;
  std::optional<int> alignment;
  std::optional<int> bit_width;
  bool no_unique_address = false;
};

consteval info data_member_spec(info type, data_member_options options);
template <class R = std::initializer_list<info>>
consteval info define_aggregate(info class_type, R&& mdescrs);

template <class T>
consteval info reflect_constant(const T& expr);
template <class T>
consteval info reflect_object(T& expr);
}  // namespace std::meta
