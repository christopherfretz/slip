// std: c++17
#include "side.h"
// §6 IfKind `Ordinary`.
#ifdef B
void f() { if (true) {} }
#else
void f() { if constexpr (true) {} }
#endif
