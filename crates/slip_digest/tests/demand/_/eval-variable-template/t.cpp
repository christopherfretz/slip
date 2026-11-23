#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 5): a constexpr variable
// template specialisation evaluated on use.
template <class T> constexpr T v = T(1);
using U = decltype(v<int>);
#ifdef B
void use() { static_assert(v<int> == 1); }
#endif
