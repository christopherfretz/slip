#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123): the explicit argument of a dependent name is an expression on one side and a type on the other.
namespace n { constexpr int k = 0; }
template <int I, class T> int get(T);
template <class U, class T> int get(T);
#ifdef B
template <class T> int f(T t) { return get<int>(t); }
#else
template <class T> int f(T t) { return get<n::k>(t); }
#endif
