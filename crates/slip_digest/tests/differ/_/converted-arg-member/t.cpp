#include "side.h"
// WP-163 fix round 1 (F1): a member's type that names a constant in templated code, n::k against n::j, both 0. Since Kiln WP-3321 the canonical type holds the converted value 0; the unit reads the written argument (own_spelling `converted_args`), so the pair differs as at the pin before.
namespace n { constexpr int k = 0; constexpr int j = 0; }
template <class T, int I> struct It {};
#ifdef B
template <class T> struct M { It<T, n::j>* p; };
#else
template <class T> struct M { It<T, n::k>* p; };
#endif
