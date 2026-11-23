#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a return type of the typedef: no alignment is read from a function's type; a use of the call's type (`alignof(decltype(fr()))`) writes it where it is used.
typedef int I __attribute__((aligned(8)));
#ifdef B
I fr();
#else
int fr();
#endif
