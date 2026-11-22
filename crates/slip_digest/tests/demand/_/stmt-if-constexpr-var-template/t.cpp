// std: c++17
#include "side.h"
// WP-034 (§6): `if constexpr` naming a variable template specialisation instantiated before on B (in a function: a variable would change the order unit).
template <class T> constexpr bool b = sizeof(T) > 1;
#ifdef B
bool g() { return b<int>; }
#endif
void f() { if constexpr (b<int>) ; }
