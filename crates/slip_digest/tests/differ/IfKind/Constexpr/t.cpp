// std: c++17
#include "side.h"
// §6 IfKind `Constexpr`.
#ifdef B
void f() { if constexpr (true) {} }
#else
void f() { if (true) {} }
#endif
