#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123, form A): the same constant named through a namespace alias. Rule 11 reads a written qualifier into the unit's list only where it can hold an instantiation-dependent operand; a namespace's cannot.
namespace n { constexpr int k = 0; }
namespace m = n;
template <int I, class T> int get(T);
#ifdef B
template <class T> int f(T t) { return get<m::k>(t); }
#else
template <class T> int f(T t) { return get<n::k>(t); }
#endif
