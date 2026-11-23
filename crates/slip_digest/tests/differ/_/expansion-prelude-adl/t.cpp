// std: c++26
#include "side.h"
// WP-034 (§6): ExpansionData `prelude`: the iterating form: the prelude's `begin` call selects another function.
#ifdef B
struct R { int a[2] = {1, 2}; };
template <class T> constexpr const int *begin(const T &r) { return r.a; }
constexpr const int *end(const R &r) { return r.a + 2; }
constexpr R r{};
void f() { template for (constexpr int e : r) ; }
#else
struct R { int a[2] = {1, 2}; };
constexpr const int *begin(const R &r) { return r.a; }
constexpr const int *end(const R &r) { return r.a + 2; }
constexpr R r{};
void f() { template for (constexpr int e : r) ; }
#endif
