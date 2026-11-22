// std: c++20
#include "side.h"
// §5.4 InitStyle `ParenList`: an aggregate initialised by a parenthesised list.
#ifdef B
struct S { int a; };
S x(1);
#else
struct S { int a; };
S x{1};
#endif
