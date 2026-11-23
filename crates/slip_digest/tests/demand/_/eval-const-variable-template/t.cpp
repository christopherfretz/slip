#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 5): a `const` variable
// template specialisation evaluated on use.
template <class T> const T c = T(2);
using U = decltype(c<int>);
#ifdef B
void use() { static_assert(c<int> == 2); }
#endif
