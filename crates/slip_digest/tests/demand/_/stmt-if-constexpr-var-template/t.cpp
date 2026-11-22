// std: c++17
#include "side.h"
// WP-034 (§6): `if constexpr` naming a variable template specialisation instantiated before on B.
template <class T> constexpr bool b = sizeof(T) > 1;
#ifdef B
bool u = b<int>;
#endif
void f() { if constexpr (b<int>) ; }
