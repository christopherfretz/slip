#include "side.h"
// digest-kinds §4.6 ArgView `Expr` (WP-123): the same concept-id as the
// argument of a specialisation a member's type names. The type's digest
// begins a profile for the value-dependent argument, and the concept-id's
// unconverted `n::k` is written in that profile's frame with the unit's
// local state put aside (`Walk::written_arg`, third case).
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int N> concept C = sizeof(T) > N;
template <bool V> struct X {};
#ifdef B
template <class T> struct M { X<C<T, n::j>> *p; };
#else
template <class T> struct M { X<C<T, n::k>> *p; };
#endif
