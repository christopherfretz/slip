#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123, form A): the explicit argument of a dependent name, a non-dependent expression no template was chosen for, names another constant. It is the unit's own expression, walked in full.
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <int I, class T> int get(T);
#ifdef B
template <class T> int f(T t) { return get<n::j>(t); }
#else
template <class T> int f(T t) { return get<n::k>(t); }
#endif
