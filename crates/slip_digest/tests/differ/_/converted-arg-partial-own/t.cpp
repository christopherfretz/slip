#include "side.h"
// WP-163 fix round 2 (B1): a class template's partial specialisation's own arguments, n::k against n::j, both 0. The converted list is P<T, 0> on both sides; the unit reads Kiln's list as written (args_as_written) against the canonical one, as rule 12 reads a written type (own_spelling `converted_args`).
namespace n { constexpr int k = 0; constexpr int j = 0; }
#ifdef B
template <class T, int I> struct P {};
template <class T> struct P<T, n::j> { int m; };
#else
template <class T, int I> struct P {};
template <class T> struct P<T, n::k> { int m; };
#endif
