#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123): the concept-id as a
// requires-clause, which the declaration's key holds: written in the
// key's profile frame with the unit's local state put aside
// (`Walk::written_arg`, third case).
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int N> concept C = sizeof(T) > N;
#ifdef B
template <class T> requires C<T, n::j> int f(T);
#else
template <class T> requires C<T, n::k> int f(T);
#endif
