#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123): a concept-id among the
// arguments of a value-dependent argument, which Kiln profiles: the
// concept-id's own list keeps `n::k` unconverted, so it is written in the
// frame of the profile the unit level began (`Walk::written_arg`, second
// case). The constant it names is another.
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int N> concept C = sizeof(T) > N;
template <bool V, class T> int getb(T);
#ifdef B
template <class T> int f(T t) { return getb<C<T, n::j>>(t); }
#else
template <class T> int f(T t) { return getb<C<T, n::k>>(t); }
#endif
