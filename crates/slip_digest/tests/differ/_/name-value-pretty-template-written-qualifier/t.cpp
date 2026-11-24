#include "side.h"
// WP-125 (WP-041 re-check of round 4, G3; digest §9): `__PRETTY_FUNCTION__`
// of a function template prints its parameter types as written, qualifiers
// included: `h(1, 1)` is 53 on side A and 55 on side B (clang-19), and
// Kiln's constant is the same (a `static_assert` of each value holds under
// Kiln, `spec/landed/digest/WP-125.md`). `sizeof` reads the array's type
// in the instantiation's body, so the instantiation's definition differs;
// the pattern holds no string (its `__PRETTY_FUNCTION__` is dependent) and
// is equal, as is `use`.
namespace ns { struct S { static constexpr int value = 1; }; }
#ifdef B
template<class T> unsigned long h(T, decltype(::ns::S::value)) { return sizeof(__PRETTY_FUNCTION__); }
#else
template<class T> unsigned long h(T, decltype(ns::S::value)) { return sizeof(__PRETTY_FUNCTION__); }
#endif
unsigned long use() { return h(1, 1); }
