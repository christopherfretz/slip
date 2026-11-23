// std: c++17
#include "side.h"
// §5.3 DeclNode `Var.data`: `inline`.
#ifdef B
inline int v = 0;
#else
int v = 0;
#endif
