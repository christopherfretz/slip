#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an alias-declaration of the typedef, and its use.
typedef int I __attribute__((aligned(8)));
#ifdef B
using U = I;
#else
using U = int;
#endif
struct S { char c; U i; };
