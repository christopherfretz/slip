#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 6): a variable whose
// pattern has an initialiser has a `definition` only once Kiln has
// instantiated the initialiser; naming it in `decltype` declares it.
template <class T> constexpr T v = T(1);
using U = decltype(v<int>);
#ifdef B
int h() { return v<int>; }
#endif
