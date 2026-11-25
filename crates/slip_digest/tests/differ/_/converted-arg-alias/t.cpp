#include "side.h"
// WP-163 fix round 1 (F1): a member alias that names a constant in templated code, n::k against n::j, both 0. Since Kiln WP-3321 the canonical type holds the converted value 0; the unit reads the written argument (own_spelling `converted_args`), so the pair differs as at the pin before.
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int I> struct It {};
#ifdef B
template <class T> struct M { using A = It<T, n::j>; };
#else
template <class T> struct M { using A = It<T, n::k>; };
#endif
